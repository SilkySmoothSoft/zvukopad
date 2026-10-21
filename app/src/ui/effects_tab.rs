//! Вкладка «Эффекты»: всё, что играет программа, проходит через включённые эффекты.

use eframe::egui::{self, RichText};
use zv_core::controls::Action;
use zv_core::effects::{EffectsSettings, effect_presets};
use zv_core::i18n::tr;

use super::settings_tab::section;
use super::{ZvApp, style};

pub fn show(app: &mut ZvApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| content(app, ui));
    });
}

fn content(app: &mut ZvApp, ui: &mut egui::Ui) {
    let before = app.settings.effects.clone();
    ui.heading(tr("Эффекты"));
    ui.label(
        RichText::new(tr(
            "Включённые эффекты применяются ко всем звукам программы (ваш голос с микрофона остаётся чистым).",
        ))
        .color(style::MUTED),
    );
    ui.add_space(6.0);
    {
        let e = &mut app.settings.effects;
        ui.horizontal(|ui| {
            let label = if e.enabled {
                tr("🎛 Эффекты ВКЛЮЧЕНЫ")
            } else {
                tr("🎛 Эффекты выключены")
            };
            let color = if e.enabled { style::PLAYING } else { style::MUTED };
            ui.toggle_value(&mut e.enabled, RichText::new(label).size(17.0).color(color));
            ui.label(
                RichText::new(format!("({})", app.settings.controls.key(Action::ToggleEffects))).color(style::MUTED),
            );
        });
        ui.horizontal(|ui| {
            ui.label(tr("Применять к:"));
            ui.checkbox(&mut e.on_monitor, tr("звуку «себе»"));
            ui.checkbox(&mut e.on_virtual, tr("звуку «в микрофон»"));
        });
    }
    ui.add_space(8.0);

    section(ui, tr("Быстрые пресеты"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for (name, preset) in effect_presets() {
                if ui.button(tr(name)).clicked() {
                    let keep = (app.settings.effects.on_monitor, app.settings.effects.on_virtual);
                    app.settings.effects = EffectsSettings { on_monitor: keep.0, on_virtual: keep.1, ..preset };
                }
            }
            if ui.button(tr("Сбросить всё")).clicked() {
                app.settings.effects = EffectsSettings::default();
            }
        });
    });
    ui.add_space(8.0);

    let e = &mut app.settings.effects;
    let pct = |v: f64, _| format!("{:.0} %", v * 100.0);
    section(ui, tr("Скорость (и тон)"), |ui| {
        ui.checkbox(&mut e.speed_on, tr("Включить"));
        ui.add_enabled_ui(e.speed_on, |ui| {
            ui.add(egui::Slider::new(&mut e.speed, 0.5..=2.0).custom_formatter(|v, _| format!("×{v:.2}")));
            ui.label(
                RichText::new(tr("Больше 1 — быстрее и выше («бурундук»), меньше — медленнее и ниже."))
                    .small()
                    .color(style::MUTED),
            );
        });
    });
    ui.add_space(6.0);
    section(ui, tr("Реверберация"), |ui| {
        ui.checkbox(&mut e.reverb_on, tr("Включить"));
        ui.add_enabled_ui(e.reverb_on, |ui| {
            egui::Grid::new("rev").num_columns(2).show(ui, |ui| {
                ui.label(tr("Размер помещения"));
                ui.add(egui::Slider::new(&mut e.reverb_room, 0.0..=1.0).custom_formatter(pct));
                ui.end_row();
                ui.label(tr("Глушение высоких"));
                ui.add(egui::Slider::new(&mut e.reverb_damp, 0.0..=1.0).custom_formatter(pct));
                ui.end_row();
                ui.label(tr("Сила эффекта"));
                ui.add(egui::Slider::new(&mut e.reverb_mix, 0.0..=1.0).custom_formatter(pct));
                ui.end_row();
            });
        });
    });
    ui.add_space(6.0);
    section(ui, tr("Эхо"), |ui| {
        ui.checkbox(&mut e.echo_on, tr("Включить"));
        ui.add_enabled_ui(e.echo_on, |ui| {
            egui::Grid::new("echo").num_columns(2).show(ui, |ui| {
                ui.label(tr("Задержка"));
                ui.add(egui::Slider::new(&mut e.echo_ms, 20.0..=1500.0).suffix(tr(" мс")));
                ui.end_row();
                ui.label(tr("Число повторов"));
                ui.add(egui::Slider::new(&mut e.echo_feedback, 0.0..=0.9).custom_formatter(pct));
                ui.end_row();
                ui.label(tr("Сила эффекта"));
                ui.add(egui::Slider::new(&mut e.echo_mix, 0.0..=1.0).custom_formatter(pct));
                ui.end_row();
            });
        });
    });
    ui.add_space(6.0);
    section(ui, tr("Окраска звука"), |ui| {
        ui.checkbox(&mut e.radio_on, tr("📻 Рация — узкая полоса частот, как из динамика рации"));
        ui.horizontal(|ui| {
            ui.checkbox(&mut e.distortion_on, tr("🔥 Перегруз"));
            ui.add_enabled(e.distortion_on, egui::Slider::new(&mut e.distortion_drive, 1.0..=30.0).text(tr("сила")));
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut e.bitcrush_on, tr("👾 8 бит"));
            ui.add_enabled(e.bitcrush_on, egui::Slider::new(&mut e.bitcrush_bits, 2.0..=12.0).text(tr("разрядов")));
            ui.add_enabled(
                e.bitcrush_on,
                egui::Slider::new(&mut e.bitcrush_downsample, 1.0..=16.0).text(tr("огрубление")),
            );
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut e.robot_on, tr("🤖 Робот"));
            ui.add_enabled(e.robot_on, egui::Slider::new(&mut e.robot_hz, 20.0..=300.0).suffix(tr(" Гц")));
        });
    });

    if app.settings.effects != before {
        // Включили что-то конкретное — значит, эффекты нужны.
        let turned_on_effect = effect_count(&app.settings.effects) > effect_count(&before);
        if turned_on_effect && !app.settings.effects.enabled {
            app.settings.effects.enabled = true;
        }
        app.engine.set_effects(&app.settings.effects);
        app.mark_settings_dirty();
    }
}

fn effect_count(e: &EffectsSettings) -> usize {
    [e.speed_on, e.reverb_on, e.echo_on, e.radio_on, e.distortion_on, e.bitcrush_on, e.robot_on]
        .iter()
        .filter(|x| **x)
        .count()
}
