//! Вкладка «Синтез речи».

use eframe::egui::{self, RichText};
use zv_core::i18n::tr;
use zv_core::tf;

use super::{ZvApp, style, widgets};
use crate::download::Job;
use crate::piper::{self, PIPER_ID};
use crate::sysutil;
use crate::tts::SynthPurpose;

pub fn show(app: &mut ZvApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| content(app, ui));
    });
}

fn content(app: &mut ZvApp, ui: &mut egui::Ui) {
    ui.heading(tr("Синтез речи"));
    ui.label(RichText::new(tr("Напишите текст — программа произнесёт его выбранным голосом.")).color(style::MUTED));
    ui.add_space(10.0);

    if app.tts.plugins.is_empty() {
        ui.label(RichText::new(tr("Не найдено ни одного плагина синтеза речи.")).color(style::DANGER));
        ui.label(tr("Встроенный плагин должен лежать в папке plugins рядом с программой."));
        plugins_footer(app, ui);
        return;
    }

    let plugin_id = app.settings.tts.plugin_id.clone();
    app.ensure_voices(&plugin_id);

    egui::Grid::new("tts_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(tr("Движок:"));
        let current =
            app.tts.plugins.iter().find(|p| p.id == plugin_id).map(|p| tr(&p.name).to_string()).unwrap_or_default();
        let mut new_plugin = None;
        egui::ComboBox::from_id_salt("tts_plugin").width(320.0).selected_text(current).show_ui(ui, |ui| {
            for p in &app.tts.plugins {
                if ui.selectable_label(p.id == plugin_id, tr(&p.name)).on_hover_text(tr(&p.description)).clicked() {
                    new_plugin = Some(p.id.clone());
                }
            }
        });
        if let Some(id) = new_plugin
            && id != plugin_id
        {
            app.settings.tts.plugin_id = id;
            app.settings.tts.voice_id.clear();
            app.mark_settings_dirty();
        }
        ui.end_row();

        ui.label(tr("Голос:"));
        ui.horizontal(|ui| {
            match app.tts_voices.get(&plugin_id).cloned() {
                None => {
                    ui.spinner();
                    ui.label(tr("Загружаю список голосов…"));
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(tf!("Ошибка: {}", e)).color(style::DANGER));
                }
                Some(Ok(voices)) if voices.is_empty() => {
                    ui.label(RichText::new(tr("Голосов не найдено")).color(style::DANGER));
                }
                Some(Ok(voices)) => {
                    let cur = voices
                        .iter()
                        .find(|v| v.id == app.settings.tts.voice_id)
                        .map(|v| v.name.clone())
                        .unwrap_or_else(|| tr("выберите голос").into());
                    egui::ComboBox::from_id_salt("tts_voice").width(320.0).selected_text(cur).show_ui(ui, |ui| {
                        for v in &voices {
                            let label = if v.language.is_empty() {
                                v.name.clone()
                            } else {
                                format!("{} ({})", v.name, v.language)
                            };
                            if ui.selectable_label(v.id == app.settings.tts.voice_id, label).clicked() {
                                app.settings.tts.voice_id = v.id.clone();
                                app.mark_settings_dirty();
                            }
                        }
                    });
                }
            }
            if ui.button("⟳").on_hover_text(tr("Перечитать голоса")).clicked() {
                app.reload_voices(&plugin_id);
            }
        });
        ui.end_row();

        ui.label(tr("Скорость:"));
        if ui.add(egui::Slider::new(&mut app.settings.tts.rate, -10..=10)).changed() {
            app.mark_settings_dirty();
        }
        ui.end_row();
    });

    ui.add_space(8.0);
    widgets::text_edit_with_menu(ui, &mut app.tts_text, true, egui::Id::unique("tts_text"), |e| {
        e.hint_text(tr("Текст для озвучки… (Ctrl+V — вставить, правый клик — меню)"))
            .desired_rows(5)
            .desired_width(f32::INFINITY)
    });
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        let busy = app.tts.busy;
        ui.add_enabled_ui(!busy, |ui| {
            if ui.button(tr("🎧 Прослушать")).on_hover_text(tr("Только себе")).clicked() {
                app.synthesize(SynthPurpose::Preview);
            }
            if ui
                .button(RichText::new(tr("🗣 Сказать")).strong())
                .on_hover_text(tf!("Туда, куда выбрано вверху: {}", app.settings.audio.target.label()))
                .clicked()
            {
                app.synthesize(SynthPurpose::Say);
            }
            if ui.button(tr("💾 Сохранить в библиотеку")).clicked() {
                app.synthesize(SynthPurpose::Save);
            }
        });
        if busy {
            ui.spinner();
            ui.label(tr("Генерирую…"));
        }
    });
    if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter)) && !app.tts.busy {
        app.synthesize(SynthPurpose::Say);
    }
    ui.label(RichText::new(tr("Ctrl+Enter — сказать")).small().color(style::MUTED));

    ui.add_space(16.0);
    piper_section(app, ui, plugin_id == PIPER_ID);

    ui.add_space(16.0);
    ui.separator();
    ui.label(RichText::new(tr("Где взять голоса")).strong());
    ui.label(tr("Голоса Windows: Параметры → Время и язык → Речь → «Добавить голоса». \
         После установки нажмите ⟳ рядом со списком голосов."));
    ui.label(tr(
        "Для очень слабых компьютеров: RHVoice (бесплатный, есть русские голоса Анна, Елена, Ирина, Александр и др.) — \
         ставится как голос Windows и сам появится в движке «Голоса Windows». Сайт: rhvoice.su",
    ));
    ui.label(
        RichText::new(tr(
            "Другие нейросетевые голоса подключаются плагинами — см. docs/PLUGINS.md в папке с исходниками.",
        ))
        .color(style::MUTED),
    );
    plugins_footer(app, ui);
}

fn plugins_footer(app: &mut ZvApp, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button(tr("📂 Папка плагинов пользователя")).clicked() {
            sysutil::open_folder(&app.paths.user_plugins_dir());
        }
        ui.label(
            RichText::new(tr("Новые плагины подхватываются при перезапуске программы.")).small().color(style::MUTED),
        );
    });
    for e in &app.tts.discovery_errors {
        ui.label(RichText::new(e).small().color(style::DANGER));
    }
}

/// Встроенный Piper: скачивание движка и голосов.
fn piper_section(app: &mut ZvApp, ui: &mut egui::Ui, selected: bool) {
    super::settings_tab::section(
        ui,
        tr("🧠 Piper — быстрые нейросетевые голоса без интернета"),
        |ui| {
            ui.label(
                RichText::new(tr("Работает на процессоре даже на слабых ПК: фраза озвучивается за доли секунды. \
                 Скачивается один раз в папку данных программы (не в папку с программой). \
                 Источники: github.com/rhasspy/piper и huggingface.co/rhasspy/piper-voices."))
                .small()
                .color(style::MUTED),
            );
            ui.add_space(4.0);

            let busy = app.downloads.busy();
            if let Some(p) = app.downloads.progress() {
                ui.horizontal(|ui| {
                    let frac = p.total.map(|t| p.done as f32 / t.max(1) as f32);
                    let mb = |b: u64| b as f64 / 1_048_576.0;
                    let text = match p.total {
                        Some(t) => {
                            tf!("{} — {} из {} МБ", p.label, format!("{:.1}", mb(p.done)), format!("{:.1}", mb(t)))
                        }
                        None if p.done > 0 => tf!("{} — {} МБ", p.label, format!("{:.1}", mb(p.done))),
                        None => p.label.clone(),
                    };
                    match frac {
                        Some(f) => {
                            ui.add(egui::ProgressBar::new(f).desired_width(260.0).show_percentage());
                        }
                        None => {
                            ui.spinner();
                        }
                    }
                    ui.label(text);
                    if ui.button(tr("Отмена")).clicked() {
                        app.downloads.cancel();
                    }
                });
                ui.add_space(4.0);
            }

            // 1. Движок
            let engine_ok = app.piper.engine_installed();
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("1. Движок:")).strong());
                if engine_ok {
                    ui.label(RichText::new(tr("✔ установлен")).color(style::ACCENT));
                    if ui.add_enabled(!busy, egui::Button::new(tr("переустановить")).small()).clicked() {
                        app.downloads.start(Job::PiperEngine);
                    }
                } else if ui
                    .add_enabled(!busy, egui::Button::new(tf!("⬇ Скачать движок (≈ {} МБ)", piper::ENGINE_SIZE_MB)))
                    .on_hover_text(piper::ENGINE_URL)
                    .clicked()
                {
                    app.downloads.start(Job::PiperEngine);
                }
            });

            // 2. Голоса
            ui.add_space(4.0);
            ui.label(RichText::new(tr("2. Русские голоса:")).strong());
            egui::Grid::new("piper_voices").num_columns(3).spacing([12.0, 4.0]).show(ui, |ui| {
                for v in piper::CATALOG {
                    ui.label(tr(v.title));
                    if app.piper.voice_installed(v.file) {
                        ui.label(RichText::new(tr("✔ скачан")).color(style::ACCENT));
                        if ui.small_button("🗑").on_hover_text(tr("Удалить голос")).clicked() {
                            let d = app.piper.voices_dir();
                            let _ = std::fs::remove_file(d.join(format!("{}.onnx", v.file)));
                            let _ = std::fs::remove_file(d.join(format!("{}.onnx.json", v.file)));
                            app.reload_voices(PIPER_ID);
                        }
                    } else {
                        if ui
                            .add_enabled(!busy, egui::Button::new(tf!("⬇ Скачать (≈ {} МБ)", v.size_mb)))
                            .on_hover_text(v.url())
                            .clicked()
                        {
                            app.downloads.start(Job::PiperVoice { url: v.url() });
                        }
                        ui.label("");
                    }
                    ui.end_row();
                }
            });

            // 3. Свой голос
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Свой голос по ссылке:")).strong());
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.voice_url_buf)
                        .desired_width(420.0)
                        .hint_text(tr("https://huggingface.co/rhasspy/piper-voices/…/имя.onnx")),
                );
                let ok = piper::normalize_voice_url(&app.voice_url_buf);
                if ui.add_enabled(!busy && ok.is_ok(), egui::Button::new(tr("⬇ Скачать"))).clicked()
                    && let Ok(url) = ok
                {
                    app.downloads.start(Job::PiperVoice { url });
                    app.voice_url_buf.clear();
                }
            });
            ui.label(
                RichText::new(tr(
                    "Ссылка на файл .onnx голоса Piper (любой язык; рядом на сайте должен быть .onnx.json). \
                 Или скачайте оба файла сами и положите в папку голосов.",
                ))
                .small()
                .color(style::MUTED),
            );
            ui.horizontal(|ui| {
                if ui.button(tr("📂 Папка голосов")).clicked() {
                    let d = app.piper.voices_dir();
                    let _ = std::fs::create_dir_all(&d);
                    sysutil::open_folder(&d);
                }
                if ui.button(tr("🌐 Все голоса Piper")).on_hover_text(tr("Список голосов с прослушиванием")).clicked()
                {
                    sysutil::open_url("https://rhasspy.github.io/piper-samples/");
                }
                if ui.button(tr("⟳ Перечитать голоса")).clicked() {
                    app.reload_voices(PIPER_ID);
                }
            });

            if !selected && engine_ok {
                ui.add_space(4.0);
                if ui.button(RichText::new(tr("Использовать Piper")).strong()).clicked() {
                    app.settings.tts.plugin_id = PIPER_ID.into();
                    app.settings.tts.voice_id.clear();
                    app.reload_voices(PIPER_ID);
                    app.mark_settings_dirty();
                }
            }
        },
    );
}
