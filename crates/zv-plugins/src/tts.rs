//! Плагины синтеза речи.
//!
//! Методы протокола:
//! * `list_voices` → `{"voices":[{"id":"...","name":"...","language":"ru-RU"}]}`
//! * `synthesize` с `{"text","voice","rate","output"}` → `{"path":"<output>"}`;
//!   плагин сам пишет WAV по пути `output`.

use std::path::{Path, PathBuf};
use std::time::Duration;
use zv_core::i18n::tr;
use zv_core::tf;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::host::PluginProcess;
use crate::manifest::PluginManifest;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TtsVoice {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct SynthRequest {
    pub text: String,
    pub voice: String,
    /// −10..10, 0 — обычная скорость.
    pub rate: i32,
    pub output: PathBuf,
}

/// Плагин TTS: процесс запускается при первом обращении и перезапускается, если упал.
pub struct TtsPlugin {
    manifest: PluginManifest,
    process: Option<PluginProcess>,
}

impl TtsPlugin {
    pub fn new(manifest: PluginManifest) -> Self {
        Self { manifest, process: None }
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn process(&mut self) -> Result<&mut PluginProcess, String> {
        let alive = self.process.as_mut().is_some_and(|p| p.is_alive());
        if !alive {
            self.process = Some(PluginProcess::spawn(&self.manifest)?);
        }
        Ok(self.process.as_mut().expect("процесс только что создан"))
    }

    pub fn list_voices(&mut self) -> Result<Vec<TtsVoice>, String> {
        // Первый запуск PowerShell или Python с нейросетью может быть долгим.
        let result = self.process()?.request("list_voices", json!({}), Duration::from_secs(60))?;
        let voices = match result.get("voices").cloned() {
            Some(Value::Array(a)) => Value::Array(a),
            // Некоторые сериализаторы (например, PowerShell) превращают массив из одного элемента в объект.
            Some(obj @ Value::Object(_)) => Value::Array(vec![obj]),
            _ => Value::Array(vec![]),
        };
        serde_json::from_value(voices).map_err(|e| tf!("плагин вернул странный список голосов: {}", e))
    }

    pub fn synthesize(&mut self, req: &SynthRequest) -> Result<PathBuf, String> {
        if let Some(dir) = req.output.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let params = json!({
            "text": req.text,
            "voice": req.voice,
            "rate": req.rate,
            "output": path_str(&req.output),
        });
        self.process()?.request("synthesize", params, Duration::from_secs(300))?;
        if !req.output.is_file() {
            return Err(tr("плагин не создал файл со звуком").into());
        }
        Ok(req.output.clone())
    }
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
