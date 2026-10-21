//! Вкладка «Справка».

use eframe::egui::{self, RichText};
use zv_core::i18n::tr;
use zv_core::tf;

use super::{ZvApp, style};

pub fn show(app: &mut ZvApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.heading(format!("{} {}", zv_core::app_title(), zv_core::APP_VERSION));
            ui.add_space(8.0);

            step(ui, tr("Как говорить звуками в Discord / Skype"), &[
                tr("1. Установите VB-Cable (бесплатно, vb-audio.com/Cable) и перезагрузите компьютер."),
                tr("2. Здесь: Настройки → Устройства → «В микрофон» выберите «CABLE Input»."),
                tr("3. Там же включите «Подмешивать мой голос» и выберите свой микрофон."),
                tr("4. В Discord: Настройки → Голос и видео → Устройство ввода: «CABLE Output»."),
                tr("5. В Discord отключите шумоподавление (Krisp) и автоматическую чувствительность — иначе звуки будут обрезаться."),
                tr("6. Вверху окна выберите «Себе и в микрофон» — тогда звук слышите и вы, и собеседники."),
            ]);
            step(ui, tr("Горячие клавиши"), &[
                tr("• Клавиша звука назначается кнопкой справа от звука на вкладке «Звуки»."),
                tr("• Клавиши Numpad и F13–F24 можно ввести вручную в карточке звука (например Ctrl+Numpad1)."),
                tr("• В списке: двойной клик по названию или F2 — переименовать, двойной клик по строке — играть, Del — удалить, правый клик — меню."),
                &tf!("• Номер звука: зажмите {}, наберите номер на нумпаде (NumLock включён), отпустите. {}+0 — повторить последний, {}+точка — отмена.", app.settings.numpad.modifier.label(), app.settings.numpad.modifier.label(), app.settings.numpad.modifier.label()),
                tr("• Все общие клавиши (стоп, пауза, громкость, оверлей…) — Настройки → Управление."),
                tr("• Случайный звук из тега: правый клик по тегу → «Назначить клавишу «случайный из тега»»."),
            ]);
            changelog(ui);
            ui.label(RichText::new(tr("Где лежат мои данные")).size(17.0).strong().color(style::ACCENT));
            let dirs = [
                (tr("• Звуки:"), app.sounds_dir.clone()),
                (tr("• Настройки и клавиши:"), app.paths.config_dir.clone()),
                (tr("• Логи:"), app.paths.logs_dir()),
                (tr("• Голоса Piper:"), app.piper.root.clone()),
            ];
            for (label, dir) in dirs {
                ui.horizontal(|ui| {
                    ui.label(label);
                    super::widgets::path_value(app, ui, &dir);
                });
            }
            ui.label(tr("• Папка программы личных данных не содержит — её можно удалить или заменить новой версией."));
            ui.add_space(12.0);
            step(ui, tr("Если что-то не работает"), &[
                tr("• Звука нет в Discord — проверьте пункты 2–5 выше и индикатор «В микрофон» вверху окна."),
                tr("• Клавиша красная — она занята другой программой, выберите другую."),
                &tf!("• В игре от имени администратора клавиши не работают — запустите {} тоже от имени администратора.", zv_core::app_title()),
                tr("• Подробности ошибок — на вкладке «Журнал» (кнопка «Копировать»)."),
            ]);
        });
    });
}

fn step(ui: &mut egui::Ui, title: &str, lines: &[&str]) {
    ui.label(RichText::new(title).size(17.0).strong().color(style::ACCENT));
    for l in lines {
        ui.label(*l);
    }
    ui.add_space(12.0);
}

/// История версий — из CHANGELOG.md (или CHANGELOG.en.md для английского интерфейса).
/// Каждая версия — сворачиваемый блок, последняя открыта.
fn changelog(ui: &mut egui::Ui) {
    let text = if zv_core::i18n::current_code() == "ru" {
        include_str!("../../../CHANGELOG.md")
    } else {
        include_str!("../../../CHANGELOG.en.md")
    };
    ui.label(RichText::new(tr("История версий")).size(17.0).strong().color(style::ACCENT));
    // Разбиваем на версии: «## 1.0.0 — дата», под ней абзацы и пункты «- …».
    let mut versions: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            versions.push((h.trim().to_string(), Vec::new()));
        } else if let Some((_, body)) = versions.last_mut() {
            if let Some(item) = line.strip_prefix("- ") {
                body.push(format!("• {}", item.trim()));
            } else if line.starts_with("  ") && !body.is_empty() {
                // продолжение пункта на следующей строке
                if let Some(last) = body.last_mut() {
                    last.push(' ');
                    last.push_str(line.trim());
                }
            } else if !line.trim().is_empty() {
                body.push(line.trim().to_string());
            }
        }
    }
    for (i, (title, body)) in versions.iter().enumerate() {
        egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt(("changelog", i)).default_open(i == 0).show(
            ui,
            |ui| {
                for l in body {
                    ui.label(l.replace('`', ""));
                }
            },
        );
    }
    ui.add_space(12.0);
}
