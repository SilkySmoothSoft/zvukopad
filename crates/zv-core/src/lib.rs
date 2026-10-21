//! Ядро Звукопада: пути к данным пользователя, настройки, библиотека звуков.
//!
//! Этот крейт ничего не знает ни про звук, ни про интерфейс — только данные.

pub mod controls;
pub mod effects;
pub mod i18n;
pub mod library;
pub mod paths;
pub mod pattern;
pub mod settings;
pub mod timers;

// ── Название программы ─────────────────────────────────────────────────────
// Чтобы переименовать программу, поменяйте эти две строки и файл `app-name.cmd`
// в корне проекта (его читают батники). Подробнее — README, раздел «Как переименовать».

/// Название латиницей. Из него строятся папки данных пользователя
/// (`Документы\Zvukopad`, `%APPDATA%\Zvukopad`, `%LOCALAPPDATA%\Zvukopad`).
/// Если поменять, программа начнёт с чистыми настройками — старые папки надо переименовать вручную.
pub const APP_NAME: &str = "Zvukopad";

/// Название для показа в интерфейсе (заголовок окна, оверлей, справка, имя схемы клавиш).
pub const APP_TITLE: &str = "Звукопад";

/// Название для показа — на языке интерфейса (перевод `APP_TITLE` берётся из файла перевода).
pub fn app_title() -> &'static str {
    i18n::tr(APP_TITLE)
}

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Записывает JSON атомарно: сначала во временный файл, потом переименовывает.
/// Так настройки не испортятся, если программа упадёт посреди записи.
pub fn save_json_atomic<T: serde::Serialize>(path: &std::path::Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// Читает JSON. Если файла нет — значение по умолчанию.
/// Если файл повреждён — сохраняет его копию рядом (`*.broken.json`) и возвращает значение по умолчанию.
pub fn load_json_or_default<T: serde::de::DeserializeOwned + Default>(path: &std::path::Path) -> T {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                log::error!("Файл {} повреждён: {e}. Использую значения по умолчанию.", path.display());
                let _ = std::fs::copy(path, path.with_extension("broken.json"));
                T::default()
            }
        },
        Err(_) => T::default(),
    }
}
