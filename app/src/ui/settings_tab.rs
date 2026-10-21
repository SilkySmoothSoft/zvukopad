//! Вкладка «Настройки».

use std::path::PathBuf;
use zv_core::i18n::tr;
use zv_core::tf;

use eframe::egui::{self, RichText};
use zv_core::controls::Action;
use zv_core::settings::{
    OverlayLayout, OverlayMode, OverlayPosition, OverlayPreset, OverlayStyle, PlayMode, builtin_overlay_presets,
};

use super::{SettingsPage, ZvApp, style, widgets};
use crate::sysutil;

pub fn show(app: &mut ZvApp, ui: &mut egui::Ui) {
    egui::Panel::left("settings_nav").resizable(false).exact_size(180.0).show(ui, |ui| {
        ui.add_space(8.0);
        let lang_label = format!("🌐 {}", widgets::language_caption());
        for (page, label) in [
            (SettingsPage::Devices, tr("🔈 Устройства")),
            (SettingsPage::Controls, tr("⌨ Управление")),
            (SettingsPage::Overlay, tr("🪟 Оверлей")),
            (SettingsPage::Folders, tr("📁 Папки")),
            (SettingsPage::Language, lang_label.as_str()),
        ] {
            ui.selectable_value(&mut app.settings_page, page, RichText::new(label).size(15.0));
        }
    });
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match app.settings_page {
            SettingsPage::Devices => {
                devices(app, ui);
                ui.add_space(10.0);
                playback(app, ui);
            }
            SettingsPage::Controls => super::controls_tab::show(app, ui),
            SettingsPage::Overlay => overlay(app, ui),
            SettingsPage::Folders => folders(app, ui),
            SettingsPage::Language => language(app, ui),
        });
    });
}

pub fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).inner_margin(10.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).size(17.0).strong());
        ui.add_space(6.0);
        body(ui);
    });
}

fn status_line(ui: &mut egui::Ui, s: &zv_audio::StreamStatus) {
    if s.active {
        ui.label(RichText::new(tf!("✔ работает, {} Гц", s.rate)).small().color(style::PLAYING));
    } else if let Some(e) = &s.error {
        ui.label(RichText::new(format!("✖ {e}")).small().color(style::DANGER));
    }
}

fn devices(app: &mut ZvApp, ui: &mut egui::Ui) {
    let devices = app.devices.clone();
    let status = app.status.clone();
    let mut changed = false;
    section(ui, tr("Устройства"), |ui| {
        egui::Grid::new("dev_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label(tr("Себе (наушники/колонки):"));
            ui.vertical(|ui| {
                let def = tf!("По умолчанию Windows ({})", devices.default_output.clone().unwrap_or_default());
                changed |= widgets::device_combo(
                    ui,
                    "dev_mon",
                    &mut app.settings.audio.monitor_device,
                    &devices.outputs,
                    &def,
                    false,
                );
                status_line(ui, &status.monitor);
            });
            ui.end_row();

            ui.label(tr("В микрофон (виртуальный кабель):"));
            ui.vertical(|ui| {
                changed |= widgets::device_combo(
                    ui,
                    "dev_virt",
                    &mut app.settings.audio.virtual_device,
                    &devices.outputs,
                    tr("Не использовать"),
                    true,
                );
                status_line(ui, &status.virtual_out);
            });
            ui.end_row();

            ui.label(tr("Мой микрофон:"));
            ui.vertical(|ui| {
                ui.add_enabled_ui(app.settings.audio.virtual_device.is_some(), |ui| {
                    changed |= ui
                        .checkbox(
                            &mut app.settings.audio.mic_passthrough,
                            tr("Подмешивать мой голос в виртуальный микрофон"),
                        )
                        .on_hover_text(tr("Без этого в Discord будут слышны только звуки, а вас — нет"))
                        .changed();
                    if app.settings.audio.mic_passthrough {
                        let def = tf!("По умолчанию Windows ({})", devices.default_input.clone().unwrap_or_default());
                        changed |= widgets::device_combo(
                            ui,
                            "dev_mic",
                            &mut app.settings.audio.mic_device,
                            &devices.inputs,
                            &def,
                            false,
                        );
                        status_line(ui, &status.mic);
                    }
                });
            });
            ui.end_row();
        });

        ui.add_space(4.0);
        if ui.button(tr("⟳ Обновить список устройств")).clicked() {
            app.refresh_devices();
            // Переоткрываем потоки: устройство могли подключить заново.
            app.engine.configure(super::engine_config(&app.settings));
        }

        let has_cable = devices.outputs.iter().any(|d| d.looks_virtual());
        ui.add_space(6.0);
        if !has_cable {
            ui.label(
                RichText::new(tr(
                    "Виртуальный кабель не найден. Чтобы звуки слышали в Discord/Skype, установите VB-Cable:",
                ))
                .color(style::ACCENT),
            );
            if ui.link(tr("vb-audio.com/Cable — официальный сайт (бесплатно)")).clicked() {
                sysutil::open_url("https://vb-audio.com/Cable/");
            }
        }
        ui.label(
            RichText::new(tr("Как подключить: здесь выберите «CABLE Input» в строке «В микрофон», \
                 а в Discord (Настройки → Голос и видео) выберите устройство ввода «CABLE Output»."))
            .small()
            .color(style::MUTED),
        );
    });
    if changed {
        app.mark_settings_dirty();
        app.apply_audio_settings();
    }
}

fn playback(app: &mut ZvApp, ui: &mut egui::Ui) {
    let mut changed = false;
    section(ui, tr("Громкость и воспроизведение"), |ui| {
        egui::Grid::new("vol_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            let a = &mut app.settings.audio;
            ui.label(tr("Громкость «себе»:"));
            changed |= widgets::volume_slider(ui, &mut a.monitor_volume, 1.5).changed();
            ui.end_row();
            ui.label(tr("Громкость «в микрофон»:"));
            changed |= widgets::volume_slider(ui, &mut a.virtual_volume, 1.5).changed();
            ui.end_row();
            ui.label(tr("Громкость моего голоса:"));
            changed |= widgets::volume_slider(ui, &mut a.mic_volume, 2.0).changed();
            ui.end_row();
            ui.label(tr("Повторное нажатие:"));
            ui.horizontal(|ui| {
                for m in [PlayMode::Restart, PlayMode::Overlap, PlayMode::Toggle] {
                    changed |= ui.radio_value(&mut a.play_mode, m, m.label()).changed();
                }
            });
            ui.end_row();
            ui.label("");
            changed |= ui
                .checkbox(&mut a.normalize, tr("Выравнивать громкость звуков"))
                .on_hover_text(tr("Тихие звуки становятся громче, громкие — тише"))
                .changed();
            ui.end_row();
        });
    });
    ui.add_space(10.0);
    section(ui, tr("🔉 Приглушение звуков, пока вы говорите"), |ui| {
        let mic_ok = app.settings.audio.mic_passthrough && app.status.mic.active;
        ui.label(
            RichText::new(
                tr("Когда вы начинаете говорить в микрофон, звуки становятся тише, чтобы вас было слышно. Работает, только если включено «Подмешивать мой голос»."),
            )
            .small()
            .color(style::MUTED),
        );
        let a = &mut app.settings.audio;
        changed |= ui.checkbox(&mut a.duck_enabled, tr("Приглушать звуки, пока я говорю")).changed();
        ui.add_enabled_ui(a.duck_enabled, |ui| {
            egui::Grid::new("duck_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label(tr("Громкость звуков, пока говорю:"));
                changed |= ui
                    .add(
                        egui::Slider::new(&mut a.duck_level, 0.0..=0.9)
                            .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                    )
                    .changed();
                ui.end_row();
                ui.label(tr("Чувствительность (порог):"));
                ui.horizontal(|ui| {
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut a.duck_threshold, 0.01..=0.5)
                                .logarithmic(true)
                                .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                        )
                        .on_hover_text(tr("Меньше — срабатывает от тихого голоса, больше — только от громкого"))
                        .changed();
                    widgets::level_meter(ui, tr("микрофон"), app.levels.mic, true);
                });
                ui.end_row();
            });
            let g = app.engine.duck_gain();
            if g < 0.98 {
                ui.label(
                    RichText::new(tf!("сейчас приглушено до {} %", format!("{:.0}", g * 100.0))).color(style::ACCENT),
                );
            }
        });
        if app.settings.audio.duck_enabled && !mic_ok {
            ui.label(RichText::new(tr("⚠ Голос не подмешивается — приглушение не сработает.")).color(style::DANGER));
        }
    });
    if changed {
        app.mark_settings_dirty();
        app.apply_audio_settings();
    }
}

fn overlay(app: &mut ZvApp, ui: &mut egui::Ui) {
    let mut changed = false;
    section(ui, tr("Когда показывать"), |ui| {
        let o = &mut app.settings.overlay;
        ui.horizontal_wrapped(|ui| {
            for m in OverlayMode::ALL {
                changed |= ui.radio_value(&mut o.mode, m, m.label()).changed();
            }
        });
        ui.horizontal(|ui| {
            ui.label(tr("«На время» и показ при звуке — секунд:"));
            changed |= ui.add(egui::Slider::new(&mut o.peek_secs, 0.5..=15.0).step_by(0.5)).changed();
        });
        changed |= ui
            .checkbox(
                &mut o.show_on_numpad,
                tr("Показывать подсказки, пока набирается номер на нумпаде (в любом режиме)"),
            )
            .changed();
        changed |= ui
            .checkbox(
                &mut o.show_on_play,
                tr("Показывать на время при запуске звука, когда окно программы не в фокусе"),
            )
            .changed();
        ui.label(
            RichText::new(tf!(
                "Клавиши: «показать/скрыть» — {}, «показать на время» — {} (Настройки → Управление).",
                key_or_dash(app.settings.controls.key(Action::ToggleOverlay)),
                key_or_dash(app.settings.controls.key(Action::PeekOverlay))
            ))
            .small()
            .color(style::MUTED),
        );
        ui.label(
            RichText::new(
                tr("Оверлей — отдельное прозрачное окно поверх других: в игру он не встраивается, поэтому античиты его не трогают. \
                 Виден в оконном режиме и в режиме «без рамки», но не в эксклюзивном полноэкранном."),
            )
            .small()
            .color(style::MUTED),
        );
    });
    ui.add_space(10.0);

    let mut apply_preset: Option<OverlayStyle> = None;
    let mut delete_preset: Option<usize> = None;
    section(ui, tr("Пресеты вида"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for p in builtin_overlay_presets() {
                if ui.button(tr(&p.name)).clicked() {
                    apply_preset = Some(p.style.clone());
                }
            }
        });
        if !app.settings.overlay.presets.is_empty() {
            ui.label(RichText::new(tr("Мои пресеты:")).small().color(style::MUTED));
            ui.horizontal_wrapped(|ui| {
                for (i, p) in app.settings.overlay.presets.iter().enumerate() {
                    let r = ui.button(format!("★ {}", p.name));
                    if r.clicked() {
                        apply_preset = Some(p.style.clone());
                    }
                    r.context_menu(|ui| {
                        if ui.button(RichText::new(tr("🗑 Удалить пресет")).color(style::DANGER)).clicked()
                        {
                            delete_preset = Some(i);
                            ui.close();
                        }
                    });
                }
            });
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.overlay_preset_name).hint_text(tr("название")).desired_width(160.0),
            );
            if ui.button(tr("💾 Сохранить текущий вид как пресет")).clicked() {
                let name = app.overlay_preset_name.trim().to_string();
                if name.is_empty() {
                    app.toast_error(tr("Введите название пресета"));
                } else {
                    let style = app.settings.overlay.style.clone();
                    app.settings.overlay.presets.retain(|p| p.name != name);
                    app.settings.overlay.presets.push(OverlayPreset { name: name.clone(), style });
                    app.overlay_preset_name.clear();
                    app.mark_settings_dirty();
                    app.toast(tf!("Пресет «{}» сохранён", name));
                }
            }
        });
        ui.label(RichText::new(tr("Правый клик по своему пресету — удалить.")).small().color(style::MUTED));
    });
    if let Some(st) = apply_preset {
        app.settings.overlay.style = st;
        // Чтобы сразу было видно результат.
        app.overlay_peek_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(4));
        changed = true;
    }
    if let Some(i) = delete_preset {
        app.settings.overlay.presets.remove(i);
        changed = true;
    }
    ui.add_space(10.0);

    section(ui, tr("Вид"), |ui| {
        let st = &mut app.settings.overlay.style;
        egui::Grid::new("ov_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label(tr("Место на экране:"));
            egui::ComboBox::from_id_salt("ov_pos").selected_text(st.position.label()).show_ui(ui, |ui| {
                for p in OverlayPosition::ALL {
                    changed |= ui.selectable_value(&mut st.position, p, p.label()).changed();
                }
            });
            ui.end_row();
            ui.label(tr("Вид:"));
            ui.horizontal(|ui| {
                for l in [OverlayLayout::List, OverlayLayout::Grid] {
                    changed |= ui.radio_value(&mut st.layout, l, l.label()).changed();
                }
            });
            ui.end_row();
            ui.label(tr("Ширина (доля экрана):"));
            changed |= ui
                .add(
                    egui::Slider::new(&mut st.width, 0.08..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                )
                .changed();
            ui.end_row();
            ui.label(tr("Непрозрачность фона:"));
            changed |= ui
                .add(
                    egui::Slider::new(&mut st.opacity, 0.0..=1.0)
                        .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                )
                .changed();
            ui.end_row();
            ui.label(tr("Размер шрифта:"));
            changed |= ui.add(egui::Slider::new(&mut st.font_size, 10.0..=32.0)).changed();
            ui.end_row();
            ui.label(tr("Звуков не больше:"));
            changed |= ui.add(egui::Slider::new(&mut st.max_items, 3..=80)).changed();
            ui.end_row();
            if st.layout == OverlayLayout::Grid {
                ui.label(tr("Колонок:"));
                changed |= ui.add(egui::Slider::new(&mut st.columns, 1..=10)).changed();
                ui.end_row();
            }
            ui.label("");
            changed |= ui.checkbox(&mut st.only_with_hotkeys, tr("Только звуки с горячими клавишами")).changed();
            ui.end_row();
            ui.label("");
            changed |= ui.checkbox(&mut st.show_codes, tr("Показывать номера нумпада")).changed();
            ui.end_row();
        });
        if ui.button(tr("👁 Показать на 4 секунды")).clicked() {
            app.overlay_peek_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(4));
        }
    });
    if changed {
        app.mark_settings_dirty();
    }
}

fn key_or_dash(k: &str) -> String {
    if k.is_empty() { tr("не назначена").into() } else { k.to_string() }
}

fn folders(app: &mut ZvApp, ui: &mut egui::Ui) {
    let mut apply_dir: Option<Option<PathBuf>> = None;
    section(ui, tr("Папки"), |ui| {
        let mut hide = app.paths_hidden();
        if ui
            .checkbox(&mut hide, tr("🙈 Скрывать пути к папкам"))
            .on_hover_text(tr("В путях есть имя пользователя Windows. Включите перед стримом или скриншотом."))
            .changed()
        {
            app.set_paths_hidden(hide);
        }
        ui.label(
            RichText::new(tr("Вместо путей будет значок «путь скрыт» (нажмите — пути снова видны везде). \
                 В журнале и сообщениях имя пользователя заменяется на •••."))
            .small()
            .color(style::MUTED),
        );
        ui.add_space(6.0);
        ui.label(tr("Папка звуков:"));
        ui.horizontal(|ui| {
            if app.paths_hidden() {
                widgets::path_value(app, ui, &app.sounds_dir.clone());
            } else {
                ui.add(egui::TextEdit::singleline(&mut app.sounds_dir_buf).desired_width(420.0));
            }
            if !app.paths_hidden() && ui.button(tr("Применить")).clicked() {
                let p = PathBuf::from(app.sounds_dir_buf.trim());
                apply_dir = Some(if p == app.paths.default_sounds_dir { None } else { Some(p) });
            }
            if ui.button(tr("По умолчанию")).clicked() {
                apply_dir = Some(None);
            }
            if ui.button(tr("Открыть")).clicked() {
                sysutil::open_folder(&app.sounds_dir);
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button(tr("Папка настроек"))
                .on_hover_text(app.mask(&app.paths.config_dir.display().to_string()))
                .clicked()
            {
                sysutil::open_folder(&app.paths.config_dir);
            }
            if ui
                .button(tr("Плагины пользователя"))
                .on_hover_text(app.mask(&app.paths.user_plugins_dir().display().to_string()))
                .clicked()
            {
                sysutil::open_folder(&app.paths.user_plugins_dir());
            }
            if ui.button(tr("Логи")).on_hover_text(app.mask(&app.paths.logs_dir().display().to_string())).clicked()
            {
                sysutil::open_folder(&app.paths.logs_dir());
            }
        });
        ui.label(
            RichText::new(tr("Все личные данные хранятся в этих папках, а не в папке программы. Программу можно спокойно удалить или обновить."))
                .small()
                .color(style::MUTED),
        );
    });
    if let Some(dir) = apply_dir {
        let target = dir.clone().unwrap_or_else(|| app.paths.default_sounds_dir.clone());
        if let Err(e) = std::fs::create_dir_all(&target) {
            app.toast_error(tf!("Не удалось открыть папку: {}", e));
            return;
        }
        app.settings.sounds_dir = dir;
        app.sounds_dir = target.clone();
        app.sounds_dir_buf = target.display().to_string();
        app.category = None;
        app.selected = None;
        app.mark_settings_dirty();
        app.rescan();
        app.toast(tf!("Папка звуков: {}", target.display()));
    }
}

fn language(app: &mut ZvApp, ui: &mut egui::Ui) {
    section(ui, &widgets::language_caption(), |ui| {
        if let Some(lang) = widgets::language_options(ui, &app.settings.ui.language.clone()) {
            app.set_language(lang, ui.ctx());
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new(tr(
                "Язык переключается сразу. По умолчанию — как в Windows; если такого перевода нет — английский.",
            ))
            .small()
            .color(style::MUTED),
        );
    });
    ui.add_space(10.0);
    section(ui, tr("Свой перевод"), |ui| {
        ui.label(
            RichText::new(tr(
                "Перевод — это файл xx.json (xx — код языка, например de), где каждой русской строке сопоставлен перевод. \
                 Возьмите за образец locales/en.json из исходников, переведите и положите в папку переводов — \
                 язык появится в списке после перезапуска программы.",
            ))
            .small()
            .color(style::MUTED),
        );
        if ui.button(tr("📂 Папка переводов")).clicked() {
            let d = app.paths.config_dir.join("lang");
            let _ = std::fs::create_dir_all(&d);
            sysutil::open_folder(&d);
        }
    });
}
