//! Журнал программы.
//!
//! * Файл `%LOCALAPPDATA%\Zvukopad\logs\zvukopad.log` (предыдущий запуск — `zvukopad.old.log`).
//! * Последние строки хранятся в памяти и показываются на вкладке «Журнал».

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

/// Сколько последних строк держать в памяти для вкладки «Журнал».
const MEMORY_LINES: usize = 1000;

#[derive(Debug, Clone)]
pub struct LogLine {
    pub time: String,
    pub level: log::Level,
    pub text: String,
}

struct FileLogger {
    file: Mutex<Option<std::fs::File>>,
    memory: Mutex<VecDeque<LogLine>>,
}

static LOGGER: std::sync::OnceLock<&'static FileLogger> = std::sync::OnceLock::new();
static LOG_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        // Подробные сообщения сторонних библиотек не нужны.
        metadata.level() <= log::Level::Info || metadata.target().starts_with("zv")
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let time = local_time();
        let text = record.args().to_string();
        let line = format!("{time} [{}] {text}\n", record.level());
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
        if let Some(f) = self.file.lock().as_mut() {
            let _ = f.write_all(line.as_bytes());
            // Ошибки сразу сбрасываем на диск — вдруг программа сейчас упадёт.
            if record.level() <= log::Level::Warn {
                let _ = f.flush();
            }
        }
        let mut mem = self.memory.lock();
        if mem.len() >= MEMORY_LINES {
            mem.pop_front();
        }
        mem.push_back(LogLine { time, level: record.level(), text });
    }

    fn flush(&self) {
        if let Some(f) = self.file.lock().as_mut() {
            let _ = f.flush();
        }
    }
}

fn local_time() -> String {
    // «2026-10-19 03-06-59» → «03:06:59»
    let ts = crate::sysutil::local_timestamp();
    ts.rsplit(' ').next().unwrap_or("").replace('-', ":")
}

pub fn init(logs_dir: &Path) {
    let _ = std::fs::create_dir_all(logs_dir);
    let base = zv_core::APP_NAME.to_lowercase();
    let path = logs_dir.join(format!("{base}.log"));
    let _ = std::fs::rename(&path, logs_dir.join(format!("{base}.old.log")));
    let file = std::fs::File::create(&path).ok();
    let _ = LOG_PATH.set(path);
    let logger: &'static FileLogger =
        Box::leak(Box::new(FileLogger { file: Mutex::new(file), memory: Mutex::new(VecDeque::new()) }));
    let _ = LOGGER.set(logger);
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    std::panic::set_hook(Box::new(|info| {
        log::error!("АВАРИЯ: {info}");
        log::logger().flush();
    }));
}

/// Копия строк журнала, которые сейчас в памяти.
pub fn recent() -> Vec<LogLine> {
    LOGGER.get().map(|l| l.memory.lock().iter().cloned().collect()).unwrap_or_default()
}

/// Сколько в памяти ошибок и предупреждений.
pub fn problem_count() -> usize {
    LOGGER.get().map(|l| l.memory.lock().iter().filter(|x| x.level <= log::Level::Warn).count()).unwrap_or(0)
}

pub fn clear() {
    if let Some(l) = LOGGER.get() {
        l.memory.lock().clear();
    }
}

pub fn file_path() -> Option<PathBuf> {
    LOG_PATH.get().cloned()
}
