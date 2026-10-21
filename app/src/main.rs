// В релизной сборке не показывать чёрное окно консоли.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod download;
mod hotkeys;
mod keys;
mod logger;
mod numpad;
mod pattern_render;
mod piper;
mod player;
mod privacy;
mod ptt;
mod queue;
mod sysutil;
mod tts;
mod ui;

use std::sync::Arc;

use eframe::egui;
use zv_core::paths::AppPaths;
use zv_core::settings::Settings;
use zv_core::{APP_NAME, APP_TITLE, APP_VERSION};

use crate::sysutil::apply_language;

fn main() -> eframe::Result {
    let paths = AppPaths::detect();
    logger::init(&paths.logs_dir());
    log::info!("{APP_TITLE} {APP_VERSION} запускается");
    log::info!("Настройки: {}", paths.config_dir.display());

    // Языки: встроенные + файлы переводов в папке lang рядом с программой и в папке настроек.
    let exe_lang = sysutil::builtin_plugins_dir().parent().map(|d| d.join("lang"));
    let user_lang = paths.config_dir.join("lang");
    let mut dirs: Vec<&std::path::Path> = vec![user_lang.as_path()];
    if let Some(d) = exe_lang.as_deref() {
        dirs.insert(0, d);
    }
    zv_core::i18n::load_extra(&dirs);

    let first_run = !paths.settings_file().exists();
    let mut settings: Settings = zv_core::load_json_or_default(&paths.settings_file());
    apply_language(&settings.ui.language);
    if first_run {
        // Первый запуск: названия папок по умолчанию — уже на языке системы.
        settings = Settings::default();
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(zv_core::app_title())
            .with_app_id(APP_NAME)
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([760.0, 500.0])
            .with_icon(Arc::new(ui::style::app_icon()))
            // Прозрачность нужна оверлею: окна программы используют общий графический контекст.
            .with_transparent(true),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    let result =
        eframe::run_native(APP_NAME, options, Box::new(move |cc| Ok(Box::new(ui::ZvApp::new(cc, paths, settings)))));
    if let Err(e) = &result {
        log::error!("Окно не запустилось: {e}");
    }
    log::info!("Программа закрыта");
    log::logger().flush();
    result
}

#[cfg(test)]
mod i18n_check {
    //! Проверка: у каждой строки `tr("…")` / `tf!("…")` в исходниках есть английский перевод.
    //! Добавили новую строку — добавьте её и в `locales/en.json`, иначе этот тест упадёт.

    fn parse_literal(s: &str) -> Option<String> {
        let mut out = String::new();
        let mut it = s.chars().peekable();
        while let Some(c) = it.next() {
            match c {
                '"' => return Some(out),
                '\\' => match it.next()? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    '\n' => {
                        while it.peek().is_some_and(|c| c.is_whitespace()) {
                            it.next();
                        }
                    }
                    other => out.push(other),
                },
                _ => out.push(c),
            }
        }
        None
    }

    fn collect(dir: &std::path::Path, found: &mut Vec<(String, String)>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                collect(&p, found);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p).unwrap();
                let text = text.split("#[cfg(test)]").next().unwrap_or("");
                for marker in ["tr(", "tf!("] {
                    for (i, _) in text.match_indices(marker) {
                        let before = text[..i].chars().last();
                        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue;
                        }
                        let rest = text[i + marker.len()..].trim_start();
                        if let Some(lit) = rest.strip_prefix('"').and_then(parse_literal)
                            && lit.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c))
                        {
                            found.push((p.display().to_string(), lit));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_string_has_english_translation() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let mut found = Vec::new();
        collect(&root.join("app/src"), &mut found);
        for c in ["zv-core", "zv-audio", "zv-plugins"] {
            collect(&root.join("crates").join(c).join("src"), &mut found);
        }
        assert!(found.len() > 300, "строк найдено подозрительно мало: {}", found.len());
        let en: std::collections::HashSet<&str> = zv_core::i18n::english_keys().into_iter().map(|(k, _)| k).collect();
        let missing: Vec<String> =
            found.iter().filter(|(_, s)| !en.contains(s.as_str())).map(|(f, s)| format!("{f}: {s:?}")).collect();
        assert!(missing.is_empty(), "нет перевода в locales/en.json:\n{}", missing.join("\n"));
    }
}
