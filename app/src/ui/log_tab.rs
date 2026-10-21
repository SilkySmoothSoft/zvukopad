//! Вкладка «Журнал»: что происходило в программе, ошибки и предупреждения.

use eframe::egui::{self, Color32, RichText};
use zv_core::i18n::tr;

use super::{ZvApp, style};
use crate::{logger, sysutil};

pub fn show(app: &mut ZvApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        let lines = logger::recent();
        ui.horizontal(|ui| {
            ui.heading(tr("Журнал"));
            ui.add_space(12.0);
            ui.checkbox(&mut app.log_only_problems, tr("Только ошибки и предупреждения"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Очистить")).on_hover_text(tr("Очистить список (файл журнала остаётся)")).clicked() {
                    logger::clear();
                }
                if ui.button(tr("📋 Копировать")).on_hover_text(tr("Скопировать журнал, чтобы прислать его")).clicked() {
                    let text: String = lines
                        .iter()
                        .filter(|l| !app.log_only_problems || l.level <= log::Level::Warn)
                        .map(|l| format!("{} [{}] {}\n", l.time, l.level, app.mask(&l.text)))
                        .collect();
                    ui.ctx().copy_text(text);
                    app.toast(tr("Журнал скопирован"));
                }
                if let Some(path) = logger::file_path()
                    && ui.button(tr("📄 Файл журнала")).on_hover_text(app.mask(&path.display().to_string())).clicked()
                {
                    sysutil::show_in_folder(&path);
                }
            });
        });
        ui.label(
            RichText::new(tr("Здесь записывается всё, что делает программа. Если что-то не работает — нажмите «Копировать» и пришлите текст."))
                .small()
                .color(style::MUTED),
        );
        ui.separator();

        let shown: Vec<_> = lines.iter().filter(|l| !app.log_only_problems || l.level <= log::Level::Warn).collect();
        if shown.is_empty() {
            ui.label(RichText::new(tr("Пусто")).color(style::MUTED));
            return;
        }
        let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
        egui::ScrollArea::vertical().auto_shrink(false).stick_to_bottom(true).show_rows(
            ui,
            row_h,
            shown.len(),
            |ui, range| {
                for l in &shown[range] {
                    let color = match l.level {
                        log::Level::Error => style::DANGER,
                        log::Level::Warn => style::ACCENT,
                        _ => Color32::from_rgb(200, 200, 210),
                    };
                    let level = match l.level {
                        log::Level::Error => tr("ОШИБКА"),
                        log::Level::Warn => tr("внимание"),
                        _ => "",
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&l.time).monospace().color(style::MUTED));
                        if !level.is_empty() {
                            ui.label(RichText::new(level).monospace().color(color));
                        }
                        let text = app.mask(&l.text);
                        ui.add(egui::Label::new(RichText::new(&text).monospace().color(color)).truncate())
                            .on_hover_text(&text);
                    });
                }
            },
        );
    });
}
