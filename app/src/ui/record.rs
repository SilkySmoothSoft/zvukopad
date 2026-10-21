//! Вкладка «Запись»: запись с микрофона или того, что звучит в колонках.

use eframe::egui::{self, RichText};
use zv_core::i18n::tr;
use zv_core::settings::DeviceRef;
use zv_core::tf;

use super::{ZvApp, style, widgets};
use crate::sysutil;

pub fn show(app: &mut ZvApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| content(app, ui));
    });
}

fn source_label(source: &Option<DeviceRef>, loopback: bool) -> String {
    match (source, loopback) {
        (None, true) => tr("🔊 Что звучит в колонках (устройство по умолчанию)").into(),
        (None, false) => tr("🎤 Микрофон по умолчанию").into(),
        (Some(d), true) => tf!("🔊 Что звучит: {}", d.name),
        (Some(d), false) => format!("🎤 {}", d.name),
    }
}

/// Выбор источника: «что звучит в устройстве» или вход. Возвращает `true`, если выбор поменялся.
fn source_combo(
    ui: &mut egui::Ui,
    id: &str,
    devices: &zv_audio::DeviceList,
    source: &mut Option<DeviceRef>,
    loopback: &mut bool,
) -> bool {
    let mut changed = false;
    let current = source_label(source, *loopback);
    egui::ComboBox::from_id_salt(id).width(460.0).selected_text(current).show_ui(ui, |ui| {
        let mut pick = |ui: &mut egui::Ui, src: Option<DeviceRef>, lb: bool| {
            let sel = source.as_ref().map(|d| &d.id) == src.as_ref().map(|d| &d.id) && *loopback == lb;
            if ui.selectable_label(sel, source_label(&src, lb)).clicked() {
                *source = src;
                *loopback = lb;
                changed = true;
            }
        };
        ui.label(RichText::new(tr("Что звучит в устройстве (браузер, игры, музыка)")).small().color(style::MUTED));
        pick(ui, None, true);
        for d in &devices.outputs {
            pick(ui, Some(d.to_ref()), true);
        }
        ui.separator();
        ui.label(RichText::new(tr("Микрофоны и другие входы")).small().color(style::MUTED));
        pick(ui, None, false);
        for d in &devices.inputs {
            pick(ui, Some(d.to_ref()), false);
        }
    });
    changed
}

/// Буфер «что сейчас было».
fn replay_section(app: &mut ZvApp, ui: &mut egui::Ui) {
    use zv_core::controls::Action;
    super::settings_tab::section(ui, tr("⏪ Буфер «что сейчас было»"), |ui| {
        ui.label(
            RichText::new(
                tr("Пока включён, программа держит в памяти последние секунды звука (на диск ничего не пишется). Случилось что-то смешное — нажмите клавишу, и эти секунды сохранятся как обычный звук."),
            )
            .small()
            .color(style::MUTED),
        );
        ui.add_space(4.0);
        let info = app.engine.replay_info();
        let mut on = app.settings.replay.enabled;
        ui.horizontal(|ui| {
            if ui.checkbox(&mut on, RichText::new(tr("Включить буфер")).strong()).changed() {
                app.set_replay(on);
            }
            if let Some(i) = &info {
                ui.label(
                    RichText::new(tf!(
                        "● слушает: {} — накоплено {} из {} с",
                        i.device,
                        format!("{:.0}", i.buffered_secs),
                        i.capacity_secs
                    ))
                    .color(style::ACCENT),
                );
            } else if app.settings.replay.enabled {
                ui.label(RichText::new(tr("запускается…")).color(style::MUTED));
            }
        });
        let mut restart = false;
        ui.horizontal(|ui| {
            ui.label(tr("Источник:"));
            let r = &mut app.settings.replay;
            restart |= source_combo(ui, "replay_src", &app.devices, &mut r.source, &mut r.loopback);
        });
        ui.horizontal(|ui| {
            ui.label(tr("Держать в памяти:"));
            let r = &mut app.settings.replay;
            let resp = ui.add(egui::Slider::new(&mut r.secs, 10..=300).suffix(tr(" с")));
            // Перезапускаем буфер, только когда ползунок отпустили.
            if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
                restart = true;
            }
            let mb = r.secs as f64 * 48_000.0 * 2.0 * 4.0 / 1_048_576.0;
            ui.label(RichText::new(tf!("≈ {} МБ памяти", format!("{:.0}", mb))).small().color(style::MUTED));
        });
        if restart {
            app.mark_settings_dirty();
            if app.settings.replay.enabled {
                app.apply_replay();
            }
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let enabled = info.is_some();
            let key = app.settings.controls.key(Action::SaveReplay).to_string();
            let hint = if key.is_empty() {
                tr("Клавишу можно назначить в Настройки → Управление").to_string()
            } else {
                tf!("Горячая клавиша: {}", key)
            };
            if ui
                .add_enabled(enabled, egui::Button::new(RichText::new(tr("💾 Сохранить всё")).color(style::ACCENT)))
                .on_hover_text(hint)
                .clicked()
            {
                app.save_replay(None);
            }
            for s in [10.0, 30.0] {
                if ui.add_enabled(enabled, egui::Button::new(tf!("последние {} с", format!("{:.0}", s)))).clicked()
                {
                    app.save_replay(Some(s));
                }
            }
            if !key.is_empty() {
                ui.label(RichText::new(tf!("клавиша: {}", key)).small().color(style::MUTED));
            }
        });
        let folder = app.sounds_dir.join(&app.settings.replay.folder);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Сохраняются в:")).small().color(style::MUTED));
            if ui.small_button(tr("Открыть")).clicked() {
                sysutil::open_folder(&folder);
            }
            widgets::path_value(app, ui, &folder);
        });
    });
}

fn content(app: &mut ZvApp, ui: &mut egui::Ui) {
    ui.heading(tr("Запись звука"));
    ui.label(
        RichText::new(tr("Запишите звук из браузера, игры или с микрофона — он сразу попадёт в библиотеку."))
            .color(style::MUTED),
    );
    ui.add_space(10.0);

    let recording = app.status.recording.clone();
    ui.add_enabled_ui(recording.is_none(), |ui| {
        ui.horizontal(|ui| {
            ui.label(tr("Источник:"));
            let r = &mut app.settings.recording;
            let changed = source_combo(ui, "rec_src", &app.devices, &mut r.source, &mut r.loopback);
            if changed {
                app.mark_settings_dirty();
            }
            if ui.button("⟳").on_hover_text(tr("Обновить список устройств")).clicked() {
                app.refresh_devices();
            }
        });
    });
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        let (text, color) = if recording.is_some() {
            (tr("⏹  Остановить запись"), style::DANGER)
        } else {
            (tr("⏺  Начать запись"), style::ACCENT)
        };
        let btn = egui::Button::new(RichText::new(text).size(18.0).color(color)).min_size(egui::vec2(220.0, 40.0));
        if ui
            .add(btn)
            .on_hover_text(tf!(
                "Горячая клавиша: {}",
                app.settings.controls.key(zv_core::controls::Action::ToggleRecording)
            ))
            .clicked()
        {
            app.toggle_recording();
        }
        if let Some(r) = &recording {
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(widgets::fmt_time(app.engine.recorded_seconds())).size(22.0).monospace());
                ui.label(RichText::new(&r.device).small().color(style::MUTED));
            });
            ui.add_space(12.0);
            widgets::level_meter(ui, tr("уровень"), app.levels.recording, true);
        }
    });
    ui.add_space(6.0);
    let folder = app.sounds_dir.join(&app.settings.recording.folder);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Записи сохраняются в:")).small().color(style::MUTED));
        if ui.small_button(tr("Открыть")).clicked() {
            sysutil::open_folder(&folder);
        }
        widgets::path_value(app, ui, &folder);
    });

    ui.add_space(14.0);
    replay_section(app, ui);

    ui.add_space(14.0);
    ui.separator();
    ui.label(RichText::new(tr("Последние записи")).strong());
    let folder_id = app.settings.recording.folder.clone();
    let mut recs: Vec<_> = app.entries.iter().filter(|e| e.category == folder_id).cloned().collect();
    recs.sort_by(|a, b| b.name.cmp(&a.name));
    if recs.is_empty() {
        ui.label(RichText::new(tr("Пока нет записей")).color(style::MUTED));
    }
    for e in recs.iter().take(15) {
        ui.horizontal(|ui| {
            let playing = app.is_playing(&e.id);
            if ui.button(if playing { "■" } else { "▶" }).clicked() {
                if playing {
                    app.player.stop(&e.id);
                } else {
                    app.player.play(&e.id, Some(zv_core::settings::PlayTarget::Monitor));
                }
            }
            if ui.link(&e.name).on_hover_text(tr("Открыть в библиотеке")).clicked() {
                app.select(Some(e.id.clone()));
                app.tab = super::Tab::Sounds;
            }
            if let Some(d) = app.player.duration(&e.id) {
                ui.label(RichText::new(widgets::fmt_duration(d)).color(style::MUTED).monospace());
            }
            if ui
                .small_button(RichText::new("🗑").color(style::DANGER))
                .on_hover_text(tr("Удалить (в Корзину)"))
                .clicked()
            {
                app.pending_delete = Some(e.id.clone());
            }
        });
    }

    ui.add_space(14.0);
    ui.label(
        RichText::new(tr("Подсказка: «Что звучит» записывает всё, что играет в выбранном устройстве. \
             Чтобы записать только браузер, выключите на время другие звуки."))
        .small()
        .color(style::MUTED),
    );
}
