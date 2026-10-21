//! Где лежат данные пользователя. В папке программы личных данных нет.
//!
//! * `%APPDATA%\Zvukopad\`        — настройки, метаданные библиотеки, плагины пользователя
//! * `Документы\Zvukopad\Sounds\` — звуки (папка на виду, её удобно бэкапить)
//! * `%LOCALAPPDATA%\Zvukopad\`   — логи и временные файлы (можно удалять)

use std::path::{Path, PathBuf};

use crate::APP_NAME;

#[derive(Debug, Clone)]
pub struct AppPaths {
    /// `%APPDATA%\Zvukopad`
    pub config_dir: PathBuf,
    /// `%LOCALAPPDATA%\Zvukopad`
    pub local_dir: PathBuf,
    /// Папка со звуками по умолчанию: `Документы\Zvukopad\Sounds`
    pub default_sounds_dir: PathBuf,
}

impl AppPaths {
    pub fn detect() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let config_base = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
        let local_base = dirs::data_local_dir().unwrap_or_else(|| config_base.clone());
        let docs = dirs::document_dir().unwrap_or_else(|| home.join("Documents"));
        Self {
            config_dir: config_base.join(APP_NAME),
            local_dir: local_base.join(APP_NAME),
            default_sounds_dir: docs.join(APP_NAME).join("Sounds"),
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn library_file(&self) -> PathBuf {
        self.config_dir.join("library.json")
    }

    /// Плагины, которые пользователь поставил сам.
    pub fn user_plugins_dir(&self) -> PathBuf {
        self.config_dir.join("plugins")
    }

    /// Паттерны редактора: `%APPDATA%\Zvukopad\patterns`.
    pub fn patterns_dir(&self) -> PathBuf {
        self.config_dir.join("patterns")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.local_dir.join("logs")
    }

    pub fn temp_dir(&self) -> PathBuf {
        self.local_dir.join("temp")
    }

    pub fn ensure_dirs(&self, sounds_dir: &Path) {
        for dir in [
            self.config_dir.clone(),
            self.user_plugins_dir(),
            self.logs_dir(),
            self.temp_dir(),
            sounds_dir.to_path_buf(),
        ] {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                log::error!("Не удалось создать папку {}: {e}", dir.display());
            }
        }
    }
}
