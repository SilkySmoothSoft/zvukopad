//! Автонажатие клавиши рации (push-to-talk).
//!
//! Пока звук играет «в микрофон», программа держит нажатой клавишу рации — так игра или
//! Discord с режимом «рация» передают звук без ручного нажатия. Когда звук закончился,
//! клавиша отпускается с небольшой задержкой.
//!
//! Нажатие делается штатной функцией Windows SendInput (как экранная клавиатура).
//! Это единственное место в программе, которое создаёт ввод; оно выключено по умолчанию.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use zv_core::i18n::tr;
use zv_core::tf;

use parking_lot::Mutex;
use zv_audio::Engine;

#[derive(Debug, Clone, Default, PartialEq)]
struct Config {
    enabled: bool,
    key: String,
    release_ms: u32,
}

pub struct AutoPtt {
    config: Arc<Mutex<Config>>,
    pressed: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl AutoPtt {
    pub fn new(engine: Engine) -> Self {
        let config = Arc::new(Mutex::new(Config::default()));
        let pressed = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (c, p, s) = (config.clone(), pressed.clone(), stop.clone());
        let _ = std::thread::Builder::new().name("zv-ptt".into()).spawn(move || run(engine, c, p, s));
        Self { config, pressed, stop }
    }

    /// Применяет настройки. Пустая клавиша = выключено.
    pub fn configure(&self, enabled: bool, key: &str, release_ms: u32) {
        *self.config.lock() = Config { enabled, key: key.to_string(), release_ms };
    }

    /// Нажата ли сейчас клавиша рации.
    pub fn is_pressed(&self) -> bool {
        self.pressed.load(Ordering::Relaxed)
    }
}

impl Drop for AutoPtt {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(40));
    }
}

/// Какую клавишу нажимать.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PttKey {
    Keyboard(u16),
    /// Боковая кнопка мыши: 1 — Mouse4, 2 — Mouse5.
    MouseX(u32),
}

fn parse_key(key: &str) -> Option<PttKey> {
    let k = key.trim().to_ascii_uppercase();
    match k.as_str() {
        "" => None,
        "MOUSE4" => Some(PttKey::MouseX(1)),
        "MOUSE5" => Some(PttKey::MouseX(2)),
        _ => {
            let vk = match k.as_str() {
                "LSHIFT" | "SHIFT" => 0xA0,
                "RSHIFT" => 0xA1,
                "LCTRL" | "CTRL" => 0xA2,
                "RCTRL" => 0xA3,
                "LALT" | "ALT" => 0xA4,
                "RALT" => 0xA5,
                _ => crate::keys::vk_code(&k)?,
            };
            // Кнопки мыши кроме боковых не нажимаем — это сломало бы управление.
            (vk > 0x06).then_some(PttKey::Keyboard(vk as u16))
        }
    }
}

/// Проверка строки клавиши для интерфейса.
pub fn validate(key: &str) -> Result<(), String> {
    if key.contains('+') {
        return Err(tr("нужна одна клавиша, без Ctrl/Alt/Shift+…").into());
    }
    parse_key(key).map(|_| ()).ok_or_else(|| tf!("не понимаю клавишу «{}»", key))
}

fn run(engine: Engine, config: Arc<Mutex<Config>>, pressed: Arc<AtomicBool>, stop: Arc<AtomicBool>) {
    let mut held: Option<PttKey> = None;
    let mut last_sound = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(15));
        let cfg = config.lock().clone();
        let want_key = if cfg.enabled { parse_key(&cfg.key) } else { None };
        // Клавишу сменили или выключили — отпускаем старую.
        if held.is_some() && held != want_key {
            if let Some(k) = held.take() {
                send(k, false);
            }
            pressed.store(false, Ordering::Relaxed);
        }
        let Some(key) = want_key else { continue };
        let playing = engine.virtual_voices() > 0;
        if playing {
            last_sound = Instant::now();
            if held.is_none() {
                send(key, true);
                held = Some(key);
                pressed.store(true, Ordering::Relaxed);
            }
        } else if held.is_some() && last_sound.elapsed() >= Duration::from_millis(cfg.release_ms as u64) {
            send(key, false);
            held = None;
            pressed.store(false, Ordering::Relaxed);
        }
    }
    if let Some(k) = held {
        send(k, false);
    }
}

#[cfg(windows)]
fn send(key: PttKey, down: bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    let mut input = INPUT::default();
    match key {
        PttKey::Keyboard(vk) => {
            let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
            // Многие игры читают скан-коды, поэтому шлём скан-код, а не только код клавиши.
            let extended =
                matches!(vk, 0x21..=0x28 | 0x2D | 0x2E | 0xA3 | 0xA5 | 0x6F | 0x5B | 0x5C | 0x5D | 0x2C | 0x90);
            let mut flags = if scan != 0 { KEYEVENTF_SCANCODE } else { 0 };
            if extended {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            if !down {
                flags |= KEYEVENTF_KEYUP;
            }
            input.r#type = INPUT_KEYBOARD;
            input.Anonymous.ki = KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 };
        }
        PttKey::MouseX(button) => {
            input.r#type = INPUT_MOUSE;
            input.Anonymous.mi = MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: button,
                dwFlags: if down { MOUSEEVENTF_XDOWN } else { MOUSEEVENTF_XUP },
                time: 0,
                dwExtraInfo: 0,
            };
        }
    }
    let sent = unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
    if sent != 1 {
        log::warn!("Рация: Windows не приняла нажатие {key:?} (возможно, окно игры запущено от администратора)");
    }
}

#[cfg(not(windows))]
fn send(key: PttKey, down: bool) {
    log::debug!("Рация (не Windows): {key:?} {}", if down { "нажата" } else { "отпущена" });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(parse_key("V"), Some(PttKey::Keyboard(0x56)));
        assert_eq!(parse_key("capslock"), Some(PttKey::Keyboard(0x14)));
        assert_eq!(parse_key("Mouse5"), Some(PttKey::MouseX(2)));
        assert_eq!(parse_key("MouseLeft"), None);
        assert!(validate("Ctrl+V").is_err());
        assert!(validate("F13").is_ok());
    }
}
