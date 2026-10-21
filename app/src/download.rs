//! Скачивание движка и голосов Piper в фоне, с прогрессом и отменой.
//!
//! Файл сначала качается во временный `*.part`, и только после успешной загрузки
//! (и проверки SHA-256, если она известна) переименовывается. Оборванная загрузка
//! не оставляет «битых» голосов.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use zv_core::i18n::tr;
use zv_core::tf;

use parking_lot::Mutex;
use sha2::{Digest, Sha256};

use crate::piper::{self, PiperPaths};

/// Что скачать.
#[derive(Debug, Clone)]
pub enum Job {
    PiperEngine,
    /// Голос: прямая ссылка на `.onnx` (рядом должен лежать `.onnx.json`).
    PiperVoice {
        url: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub label: String,
    pub done: u64,
    pub total: Option<u64>,
}

pub struct Downloader {
    paths: PiperPaths,
    progress: Arc<Mutex<Option<Progress>>>,
    results: Arc<Mutex<Vec<Result<String, String>>>>,
    cancel: Arc<AtomicBool>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Downloader {
    pub fn new(paths: PiperPaths, repaint: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            paths,
            progress: Arc::new(Mutex::new(None)),
            results: Arc::new(Mutex::new(Vec::new())),
            cancel: Arc::new(AtomicBool::new(false)),
            repaint: Arc::new(repaint),
        }
    }

    /// Идёт ли загрузка.
    pub fn progress(&self) -> Option<Progress> {
        self.progress.lock().clone()
    }

    pub fn busy(&self) -> bool {
        self.progress.lock().is_some()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Результаты завершённых загрузок: `Ok(что скачано)` или `Err(ошибка)`.
    pub fn take_results(&self) -> Vec<Result<String, String>> {
        std::mem::take(&mut *self.results.lock())
    }

    pub fn start(&self, job: Job) {
        if self.busy() {
            return;
        }
        self.cancel.store(false, Ordering::Relaxed);
        *self.progress.lock() = Some(Progress { label: tr("подключаюсь…").into(), ..Default::default() });
        let (paths, progress, results, cancel, repaint) = (
            self.paths.clone(),
            self.progress.clone(),
            self.results.clone(),
            self.cancel.clone(),
            self.repaint.clone(),
        );
        std::thread::Builder::new()
            .name("zv-download".into())
            .spawn(move || {
                let ctx = Ctx { progress: progress.clone(), cancel, repaint: repaint.clone() };
                let res = match &job {
                    Job::PiperEngine => install_engine(&paths, &ctx),
                    Job::PiperVoice { url } => install_voice(&paths, url, &ctx),
                };
                match &res {
                    Ok(m) => log::info!("Загрузка: {m}"),
                    Err(e) => log::error!("Загрузка не удалась ({job:?}): {e}"),
                }
                *progress.lock() = None;
                results.lock().push(res);
                repaint();
            })
            .expect("не удалось запустить поток загрузки");
    }
}

struct Ctx {
    progress: Arc<Mutex<Option<Progress>>>,
    cancel: Arc<AtomicBool>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_connect(Some(Duration::from_secs(20)))
        .user_agent(format!("{}/{}", zv_core::APP_NAME, zv_core::APP_VERSION))
        .build()
        .into()
}

/// Качает `url` в `dest` через временный файл. Возвращает SHA-256 в hex.
fn fetch(agent: &ureq::Agent, url: &str, dest: &Path, label: &str, ctx: &Ctx) -> Result<String, String> {
    log::info!("Скачиваю {url}");
    let mut resp = agent.get(url).call().map_err(|e| match e {
        ureq::Error::StatusCode(404) => tr("файл не найден на сервере (404)").to_string(),
        ureq::Error::StatusCode(c) => tf!("сервер ответил ошибкой {}", c),
        other => tf!("нет связи: {}", other),
    })?;
    let total = resp.body().content_length();
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let part = dest.with_extension(format!(
        "{}.part",
        dest.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()
    ));
    let mut file = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let mut reader = resp.body_mut().as_reader();
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    let mut last_repaint = std::time::Instant::now();
    let result = loop {
        if ctx.cancel.load(Ordering::Relaxed) {
            break Err(tr("загрузка отменена").to_string());
        }
        let n = match reader.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => n,
            Err(e) => break Err(tf!("обрыв связи: {}", e)),
        };
        if let Err(e) = file.write_all(&buf[..n]) {
            break Err(tf!("не удалось записать файл: {}", e));
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        *ctx.progress.lock() = Some(Progress { label: label.to_string(), done, total });
        if last_repaint.elapsed() > Duration::from_millis(100) {
            last_repaint = std::time::Instant::now();
            (ctx.repaint)();
        }
    };
    drop(file);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    if let Some(t) = total
        && done != t
    {
        let _ = std::fs::remove_file(&part);
        return Err(tf!("скачано {} байт из {}", done, t));
    }
    let _ = std::fs::remove_file(dest);
    std::fs::rename(&part, dest).map_err(|e| e.to_string())?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn install_engine(paths: &PiperPaths, ctx: &Ctx) -> Result<String, String> {
    let agent = agent();
    let zip_path = paths.root.join("piper_windows_amd64.zip");
    let hash = fetch(&agent, piper::ENGINE_URL, &zip_path, tr("движок Piper"), ctx)?;
    if !hash.eq_ignore_ascii_case(piper::ENGINE_SHA256) {
        let _ = std::fs::remove_file(&zip_path);
        return Err(tf!(
            "контрольная сумма не совпала — файл повреждён или подменён.\nОжидалось {}\nПолучено  {}",
            piper::ENGINE_SHA256,
            hash
        ));
    }
    *ctx.progress.lock() = Some(Progress { label: tr("распаковываю…").into(), ..Default::default() });
    (ctx.repaint)();
    let engine_dir = paths.engine_dir();
    let _ = std::fs::remove_dir_all(&engine_dir);
    extract_zip(&zip_path, &paths.root)?;
    let _ = std::fs::remove_file(&zip_path);
    if !paths.exe().is_file() {
        return Err(tr("в архиве не оказалось piper.exe").into());
    }
    Ok(tr("движок Piper установлен").into())
}

/// Распаковывает zip. Пути с `..` и абсолютные отбрасываются.
fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(f).map_err(|e| tf!("архив повреждён: {}", e))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else { continue };
        let out: PathBuf = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut w = std::fs::File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        std::io::copy(&mut entry, &mut w).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    Ok(())
}

fn install_voice(paths: &PiperPaths, url: &str, ctx: &Ctx) -> Result<String, String> {
    let url = piper::normalize_voice_url(url)?;
    let file = url.rsplit('/').next().unwrap_or("voice.onnx");
    let stem = file.strip_suffix(".onnx").unwrap_or(file);
    let safe = zv_core::library::sanitize_file_name(stem);
    let agent = agent();
    let dir = paths.voices_dir();
    // Сначала маленький файл настроек: если его нет, большой не качаем зря.
    let json_dest = dir.join(format!("{safe}.onnx.json"));
    let try_fetch = |u: &str, dest: &Path, label: &str| fetch(&agent, u, dest, label, ctx);
    let mut base = url.clone();
    if let Err(e) = try_fetch(&format!("{base}.json"), &json_dest, &tf!("{}: настройки", safe)) {
        // У официальных голосов есть запасной адрес — ветка main.
        if base.contains("/resolve/v1.0.0/") && e.contains("404") {
            base = base.replace("/resolve/v1.0.0/", "/resolve/main/");
            try_fetch(&format!("{base}.json"), &json_dest, &tf!("{}: настройки", safe))?;
        } else {
            return Err(e);
        }
    }
    let onnx_dest = dir.join(format!("{safe}.onnx"));
    if let Err(e) = try_fetch(&base, &onnx_dest, &tf!("голос {}", safe)) {
        let _ = std::fs::remove_file(&json_dest);
        return Err(e);
    }
    Ok(tf!("голос «{}» скачан", safe))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Настоящая загрузка движка (≈ 22 МБ). Запуск: cargo test -- --ignored real_engine
    #[test]
    #[ignore]
    fn real_engine_download() {
        let dir = std::env::temp_dir().join(format!("zv-dl-{}", std::process::id()));
        let paths = PiperPaths { root: dir.clone() };
        let ctx = Ctx {
            progress: Arc::new(Mutex::new(None)),
            cancel: Arc::new(AtomicBool::new(false)),
            repaint: Arc::new(|| {}),
        };
        // На Linux install_engine вернёт ошибку (ищет `piper`, а не `piper.exe`),
        // но загрузка, проверка SHA-256 и распаковка к этому моменту уже прошли.
        let res = install_engine(&paths, &ctx);
        assert!(dir.join("piper").join("piper.exe").is_file(), "{res:?}");
        assert!(dir.join("piper").join("espeak-ng.dll").is_file(), "{res:?}");
        assert!(cfg!(not(windows)) || res.is_ok(), "{res:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
