//! Описание плагина (`plugin.json`) и поиск плагинов по папкам.

use std::path::{Path, PathBuf};
use zv_core::tf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginKind {
    /// Синтез речи из текста.
    Tts,
    /// Тип, который эта версия программы не знает (плагин будет пропущен).
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Уникальный идентификатор, латиницей: `windows-sapi`, `piper-tts`.
    pub id: String,
    /// Название для показа.
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "type")]
    pub kind: PluginKind,
    /// Программа для запуска. Путь относительно папки плагина или имя программы из PATH.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_protocol")]
    pub protocol: u32,
    /// Папка плагина (заполняется при загрузке).
    #[serde(skip)]
    pub dir: PathBuf,
}

fn default_protocol() -> u32 {
    1
}

impl PluginManifest {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let file = dir.join("plugin.json");
        let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let text = text.trim_start_matches('\u{feff}'); // BOM от Блокнота
        let mut m: PluginManifest = serde_json::from_str(text).map_err(|e| format!("{}: {e}", file.display()))?;
        if m.id.trim().is_empty() {
            return Err(tf!("{}: пустой id", file.display()));
        }
        m.dir = dir.to_path_buf();
        Ok(m)
    }

    /// Полный путь к программе плагина.
    pub fn resolved_command(&self) -> PathBuf {
        let local = self.dir.join(&self.command);
        if self.command.contains('/') || self.command.contains('\\') || local.exists() {
            local
        } else {
            PathBuf::from(&self.command)
        }
    }
}

/// Ищет плагины в папках (в каждой — подпапки с `plugin.json`).
/// Если один id встречается несколько раз, побеждает плагин из более поздней папки
/// (так плагин пользователя может заменить встроенный).
pub fn discover(roots: &[PathBuf]) -> (Vec<PluginManifest>, Vec<String>) {
    let mut found: Vec<PluginManifest> = Vec::new();
    let mut errors = Vec::new();
    for root in roots {
        let Ok(read) = std::fs::read_dir(root) else { continue };
        let mut dirs: Vec<PathBuf> =
            read.flatten().map(|e| e.path()).filter(|p| p.join("plugin.json").is_file()).collect();
        dirs.sort();
        for dir in dirs {
            match PluginManifest::load(&dir) {
                Ok(m) if m.kind == PluginKind::Unknown => {
                    errors.push(tf!("{}: неизвестный тип плагина", dir.display()));
                }
                Ok(m) if m.protocol > crate::PROTOCOL_VERSION => {
                    errors.push(tf!("{}: нужна более новая версия программы (протокол {})", m.name, m.protocol));
                }
                Ok(m) => {
                    found.retain(|x| x.id != m.id);
                    found.push(m);
                }
                Err(e) => errors.push(e),
            }
        }
    }
    (found, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest_and_override() {
        let base = std::env::temp_dir().join(format!("zv-plug-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let a = base.join("builtin").join("tts1");
        let b = base.join("user").join("tts1-mine");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("plugin.json"), r#"{"id":"tts1","name":"A","type":"tts","command":"powershell.exe"}"#)
            .unwrap();
        std::fs::write(
            b.join("plugin.json"),
            "\u{feff}{\"id\":\"tts1\",\"name\":\"B\",\"type\":\"tts\",\"command\":\"python/python.exe\"}",
        )
        .unwrap();
        let (list, errors) = discover(&[base.join("builtin"), base.join("user")]);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "B");
        assert!(list[0].resolved_command().ends_with("python/python.exe"));
        std::fs::remove_dir_all(&base).unwrap();
    }
}
