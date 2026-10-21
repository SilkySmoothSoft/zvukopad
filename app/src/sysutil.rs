//! Мелкие системные функции: открыть папку, удалить в корзину, найти встроенные плагины.

use std::path::{Path, PathBuf};
#[cfg_attr(not(windows), allow(unused_imports))]
use zv_core::tf;

/// Открывает папку в Проводнике.
pub fn open_folder(path: &Path) {
    let _ = std::fs::create_dir_all(path);
    #[cfg(windows)]
    let r = std::process::Command::new("explorer.exe").arg(path).spawn();
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = r {
        log::error!("Не удалось открыть папку {}: {e}", path.display());
    }
}

/// Открывает папку с выделенным файлом.
pub fn show_in_folder(file: &Path) {
    #[cfg(windows)]
    {
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(file.as_os_str());
        if let Err(e) = std::process::Command::new("explorer.exe").arg(arg).spawn() {
            log::error!("Не удалось открыть Проводник: {e}");
        }
    }
    #[cfg(not(windows))]
    if let Some(dir) = file.parent() {
        open_folder(dir);
    }
}

/// Открывает ссылку в браузере.
pub fn open_url(url: &str) {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer.exe").arg(url).spawn();
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(e) = r {
        log::error!("Не удалось открыть ссылку: {e}");
    }
}

/// Удаляет файл в Корзину (в Windows его можно восстановить).
pub fn delete_to_recycle_bin(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::{
            FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW, SHFileOperationW,
        };
        // Путь должен заканчиваться двумя нулями.
        let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
        from.push(0);
        from.push(0);
        let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
        op.wFunc = FO_DELETE;
        op.pFrom = from.as_ptr();
        op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as _;
        let code = unsafe { SHFileOperationW(&mut op) };
        if code != 0 {
            return Err(tf!("не удалось удалить файл (код {})", code));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}

/// Папка встроенных плагинов: `plugins` рядом с программой.
/// При запуске из исходников (`cargo run`) — `plugins` в корне проекта.
pub fn builtin_plugins_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let p = dir.join("plugins");
        if p.is_dir() {
            return p;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("plugins")
}

/// Местное время для имён файлов: `2026-10-18 14-30-05`.
pub fn local_timestamp() -> String {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::SYSTEMTIME;
        use windows_sys::Win32::System::SystemInformation::GetLocalTime;
        let mut t: SYSTEMTIME = unsafe { std::mem::zeroed() };
        unsafe { GetLocalTime(&mut t) };
        format!("{:04}-{:02}-{:02} {:02}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
    }
    #[cfg(not(windows))]
    {
        // Вне Windows — время UTC (программа рассчитана на Windows).
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let days = secs / 86_400;
        let (y, m, d) = civil_from_days(days as i64);
        let rem = secs % 86_400;
        format!("{y:04}-{m:02}-{d:02} {:02}-{:02}-{:02}", rem / 3600, (rem / 60) % 60, rem % 60)
    }
}

#[cfg(not(windows))]
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Сколько секунд прошло с местной полуночи.
pub fn local_secs_of_day() -> u32 {
    let ts = local_timestamp(); // «2026-10-19 14-30-05»
    let time = ts.rsplit(' ').next().unwrap_or("0-0-0");
    let mut it = time.split('-').map(|x| x.parse::<u32>().unwrap_or(0));
    let (h, m, s) = (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0));
    h * 3600 + m * 60 + s
}

/// Язык интерфейса Windows как код (`ru`, `en`, `de`…).
#[cfg(windows)]
pub fn system_language() -> String {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    let langid = unsafe { GetUserDefaultUILanguage() };
    // Младшие 10 бит — основной язык (PRIMARYLANGID).
    let code = match langid & 0x3ff {
        0x19 => "ru",
        0x09 => "en",
        0x22 => "uk",
        0x23 => "be",
        0x3f => "kk",
        0x07 => "de",
        0x0c => "fr",
        0x0a => "es",
        0x10 => "it",
        0x15 => "pl",
        0x16 => "pt",
        0x1f => "tr",
        0x05 => "cs",
        0x04 => "zh",
        0x11 => "ja",
        0x12 => "ko",
        _ => "en",
    };
    code.to_string()
}

/// Язык системы из переменных окружения (не Windows).
#[cfg(not(windows))]
pub fn system_language() -> String {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .ok()
        .and_then(|v| v.get(..2).map(str::to_lowercase))
        .filter(|c| c.chars().all(|ch| ch.is_ascii_alphabetic()))
        .unwrap_or_else(|| "en".into())
}

/// Включает язык из настройки (`auto` — как в системе). Возвращает код включённого языка.
pub fn apply_language(setting: &str) -> String {
    let code = if setting.is_empty() || setting == "auto" {
        zv_core::i18n::resolve_auto(&system_language())
    } else {
        setting.to_string()
    };
    zv_core::i18n::set_language(&code);
    let code = zv_core::i18n::current_code();
    log::info!("Язык интерфейса: {code} (настройка: {setting}, система: {})", system_language());
    code
}
