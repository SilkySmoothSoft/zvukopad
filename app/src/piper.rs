//! Встроенный локальный синтез речи Piper (https://github.com/rhasspy/piper).
//!
//! Piper — маленькая программа (≈ 22 МБ), которая быстро говорит нейросетевыми голосами
//! прямо на процессоре, даже на слабых компьютерах. Звукопад её не содержит: по кнопке
//! скачивает официальный выпуск с GitHub и голоса с Hugging Face (адреса — ниже и в SOURCES.md).
//!
//! Где лежит (не в папке программы):
//!   %LOCALAPPDATA%\Zvukopad\tts\piper\piper\piper.exe  — движок
//!   %LOCALAPPDATA%\Zvukopad\tts\piper\voices\*.onnx     — голоса (+ рядом *.onnx.json)
//!
//! Свой голос: положите пару файлов `имя.onnx` и `имя.onnx.json` в папку voices.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use zv_core::i18n::tr;
use zv_core::tf;

use zv_plugins::{PluginKind, PluginManifest, SynthRequest, TtsVoice};

pub const PIPER_ID: &str = "piper-builtin";

/// Официальный выпуск Piper для Windows (проверено: размер и SHA-256).
pub const ENGINE_URL: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip";
pub const ENGINE_SHA256: &str = "f3c58906402b24f3a96d92145f58acba6d86c9b5db896d207f78dc80811efcea";
pub const ENGINE_SIZE_MB: u32 = 22;

/// Официальные голоса Piper: https://huggingface.co/rhasspy/piper-voices
const VOICES_BASE: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0";

pub struct CatalogVoice {
    /// Имя файла без расширения.
    pub file: &'static str,
    pub title: &'static str,
    /// Путь внутри репозитория голосов.
    pub repo_path: &'static str,
    pub size_mb: u32,
}

impl CatalogVoice {
    pub fn url(&self) -> String {
        format!("{VOICES_BASE}/{}/{}.onnx", self.repo_path, self.file)
    }
}

/// Русские голоса из официального набора (качество medium — лучший баланс скорости и звучания).
pub const CATALOG: &[CatalogVoice] = &[
    CatalogVoice {
        file: "ru_RU-irina-medium",
        title: "Ирина — женский",
        repo_path: "ru/ru_RU/irina/medium",
        size_mb: 63,
    },
    CatalogVoice {
        file: "ru_RU-denis-medium",
        title: "Денис — мужской",
        repo_path: "ru/ru_RU/denis/medium",
        size_mb: 63,
    },
    CatalogVoice {
        file: "ru_RU-dmitri-medium",
        title: "Дмитрий — мужской",
        repo_path: "ru/ru_RU/dmitri/medium",
        size_mb: 63,
    },
    CatalogVoice {
        file: "ru_RU-ruslan-medium",
        title: "Руслан — мужской",
        repo_path: "ru/ru_RU/ruslan/medium",
        size_mb: 63,
    },
];

/// Пути Piper в папке данных программы.
#[derive(Debug, Clone)]
pub struct PiperPaths {
    pub root: PathBuf,
}

impl PiperPaths {
    pub fn new(local_dir: &Path) -> Self {
        Self { root: local_dir.join("tts").join("piper") }
    }

    pub fn engine_dir(&self) -> PathBuf {
        self.root.join("piper")
    }

    pub fn exe(&self) -> PathBuf {
        self.engine_dir().join(if cfg!(windows) { "piper.exe" } else { "piper" })
    }

    pub fn voices_dir(&self) -> PathBuf {
        self.root.join("voices")
    }

    pub fn engine_installed(&self) -> bool {
        self.exe().is_file()
    }

    pub fn voice_installed(&self, file: &str) -> bool {
        let d = self.voices_dir();
        d.join(format!("{file}.onnx")).is_file() && d.join(format!("{file}.onnx.json")).is_file()
    }
}

/// Описание встроенного движка — показывается в списке движков рядом с плагинами.
pub fn manifest(paths: &PiperPaths) -> PluginManifest {
    PluginManifest {
        id: PIPER_ID.into(),
        name: tr("Piper — нейросетевые голоса (локально)").into(),
        version: "2023.11.14-2".into(),
        description: tr("Быстрый синтез речи без интернета. Движок и голоса скачиваются по кнопке ниже.").into(),
        kind: PluginKind::Tts,
        command: String::new(),
        args: Vec::new(),
        protocol: 1,
        dir: paths.root.clone(),
    }
}

/// Голоса — пары `*.onnx` + `*.onnx.json` в папке voices.
pub fn list_voices(paths: &PiperPaths) -> Result<Vec<TtsVoice>, String> {
    if !paths.engine_installed() {
        return Err(tr("движок Piper не скачан — нажмите «Скачать движок» ниже").into());
    }
    let dir = paths.voices_dir();
    let mut voices = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("onnx")) {
                let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let cfg = p.with_extension("onnx.json");
                if !cfg.is_file() {
                    log::warn!("Piper: у голоса {stem} нет файла {stem}.onnx.json — пропускаю");
                    continue;
                }
                let language = std::fs::read_to_string(&cfg)
                    .ok()
                    .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                    .and_then(|v| v["language"]["code"].as_str().map(str::to_string))
                    .unwrap_or_default();
                let name =
                    CATALOG.iter().find(|c| c.file == stem).map(|c| tr(c.title).to_string()).unwrap_or(stem.clone());
                voices.push(TtsVoice { id: stem, name, language });
            }
        }
    }
    voices.sort_by(|a, b| a.name.cmp(&b.name));
    if voices.is_empty() {
        return Err(tr("голосов нет — скачайте голос ниже или положите свой в папку голосов").into());
    }
    Ok(voices)
}

/// Озвучивает текст в WAV.
pub fn synthesize(paths: &PiperPaths, req: &SynthRequest) -> Result<PathBuf, String> {
    let exe = paths.exe();
    if !exe.is_file() {
        return Err(tr("движок Piper не скачан").into());
    }
    let mut voice = req.voice.clone();
    if voice.is_empty() {
        voice = list_voices(paths)?.first().map(|v| v.id.clone()).unwrap_or_default();
    }
    let model = paths.voices_dir().join(format!("{voice}.onnx"));
    if !model.is_file() {
        return Err(tf!("голос «{}» не найден", voice));
    }
    if let Some(dir) = req.output.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // Piper читает текст построчно и для каждой строки перезаписывает файл, поэтому одна строка.
    let text: String = req.text.split_whitespace().collect::<Vec<_>>().join(" ");
    let length_scale = (1.0 - req.rate.clamp(-10, 10) as f32 * 0.05).clamp(0.5, 1.5);

    let mut cmd = Command::new(&exe);
    cmd.current_dir(paths.engine_dir())
        .arg("--model")
        .arg(&model)
        .arg("--output_file")
        .arg(&req.output)
        .arg("--length_scale")
        .arg(format!("{length_scale:.2}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let started = Instant::now();
    let mut child = cmd.spawn().map_err(|e| tf!("не удалось запустить Piper: {}", e))?;
    {
        let mut stdin = child.stdin.take().ok_or(tr("нет связи с Piper"))?;
        stdin.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        stdin.write_all(b"\n").map_err(|e| e.to_string())?;
    }
    // Ждём с ограничением по времени, чтобы зависший процесс не держал очередь.
    let limit = Duration::from_secs(30 + text.len() as u64 / 20);
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            return Err(tr("Piper не ответил вовремя").into());
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        use std::io::Read;
        let _ = e.read_to_string(&mut err);
    }
    if !status.success() || !req.output.is_file() {
        let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").to_string();
        return Err(tf!("Piper завершился с ошибкой ({}). {}", status, last));
    }
    log::info!("Piper: «{voice}», {} символов за {:.1} с", text.chars().count(), started.elapsed().as_secs_f32());
    Ok(req.output.clone())
}

/// Из ссылки на голос (страница или файл на Hugging Face) делает прямую ссылку на `.onnx`.
pub fn normalize_voice_url(url: &str) -> Result<String, String> {
    let mut u = url.trim().to_string();
    if !u.starts_with("https://") {
        return Err(tr("нужна ссылка, начинающаяся с https://").into());
    }
    u = u.replace("/blob/", "/resolve/");
    if let Some(q) = u.find('?') {
        u.truncate(q);
    }
    if let Some(base) = u.strip_suffix(".onnx.json") {
        u = format!("{base}.onnx");
    }
    if !u.ends_with(".onnx") {
        return Err(tr("ссылка должна вести на файл .onnx (рядом должен лежать .onnx.json)").into());
    }
    Ok(u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(
            CATALOG[0].url(),
            "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/ru/ru_RU/irina/medium/ru_RU-irina-medium.onnx"
        );
        assert_eq!(
            normalize_voice_url("https://huggingface.co/x/y/blob/main/a/b.onnx.json?download=true").unwrap(),
            "https://huggingface.co/x/y/resolve/main/a/b.onnx"
        );
        assert!(normalize_voice_url("http://x/b.onnx").is_err());
        assert!(normalize_voice_url("https://x/b.bin").is_err());
    }
}
