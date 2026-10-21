//! Плагины Звукопада.
//!
//! Плагин — это папка с файлом `plugin.json` и любой программой, которая общается со
//! Звукопадом через stdin/stdout строками JSON (по одной на строку). Язык не важен:
//! PowerShell, Python, Node, exe. Упавший плагин не роняет программу.
//!
//! Протокол описан в `docs/PLUGINS.md`.

mod host;
mod manifest;
pub mod tts;

pub use host::PluginProcess;
pub use manifest::{PluginKind, PluginManifest, discover};
pub use tts::{SynthRequest, TtsPlugin, TtsVoice};

/// Версия протокола. Плагин указывает её в `plugin.json` (`"protocol": 1`).
pub const PROTOCOL_VERSION: u32 = 1;
