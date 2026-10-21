//! Глобальные горячие клавиши (работают, даже когда окно свёрнуто или открыта игра).
//!
//! * Клавиатура — штатная функция Windows `RegisterHotKey` (через библиотеку global-hotkey).
//!   Нажатие «съедается» и не доходит до игры.
//! * Кнопки мыши и режим «пропускать нажатия в игру» — опрос состояния клавиш
//!   (`GetAsyncKeyState`) каждые 10 мс. Нажатие доходит до игры как обычно.
//!
//! Ни перехватчиков клавиатуры/мыши, ни подмены ввода — для античитов это обычная программа.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use zv_core::i18n::tr;
use zv_core::tf;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use parking_lot::Mutex;
use zv_core::controls::Action;

use crate::keys::KeyCombo;

/// Что делает клавиша.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bind {
    Sound(String),
    Action(Action),
    /// Цифра нумпада при наборе номера.
    NumDigit(u8),
    /// Отмена набора номера.
    NumCancel,
    /// Случайный звук из набора (тег, список звуков).
    RandomSet(String, std::sync::Arc<Vec<String>>),
}

/// Обработчик нажатий (вызывается из потока окна или из потока опроса).
pub type Dispatch = Arc<dyn Fn(Bind) + Send + Sync>;

struct PollEntry {
    combo: KeyCombo,
    vk: i32,
    bind: Bind,
}

#[derive(Default)]
struct PollShared {
    entries: Mutex<Vec<PollEntry>>,
    version: std::sync::atomic::AtomicU64,
    paused: AtomicBool,
}

pub struct HotkeyService {
    manager: Option<GlobalHotKeyManager>,
    registered: Vec<HotKey>,
    map: Arc<Mutex<HashMap<u32, Bind>>>,
    /// Последний набор привязок (чтобы восстановить после паузы).
    bindings: Vec<(String, Bind)>,
    pass_through: bool,
    poll: Arc<PollShared>,
    /// Ошибки регистрации: строка клавиши → причина.
    pub errors: HashMap<String, String>,
    pub init_error: Option<String>,
    paused: bool,
}

impl HotkeyService {
    /// Создаётся в главном потоке (там, где крутится цикл сообщений окна).
    pub fn new(dispatch: Dispatch) -> Self {
        let map: Arc<Mutex<HashMap<u32, Bind>>> = Arc::new(Mutex::new(HashMap::new()));
        let (manager, init_error) = match GlobalHotKeyManager::new() {
            Ok(m) => (Some(m), None),
            Err(e) => {
                log::error!("Горячие клавиши недоступны: {e}");
                (None, Some(tf!("Горячие клавиши недоступны: {}", e)))
            }
        };
        let handler_map = map.clone();
        let d = dispatch.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state() != HotKeyState::Pressed {
                return;
            }
            let bind = handler_map.lock().get(&event.id()).cloned();
            if let Some(b) = bind {
                d(b);
            }
        }));
        let poll = Arc::new(PollShared::default());
        if cfg!(windows) {
            let p = poll.clone();
            std::thread::Builder::new()
                .name("zv-key-poll".into())
                .spawn(move || poll_loop(p, dispatch))
                .expect("не удалось запустить опрос клавиш");
        }
        Self {
            manager,
            registered: Vec::new(),
            map,
            bindings: Vec::new(),
            pass_through: false,
            poll,
            errors: HashMap::new(),
            init_error,
            paused: false,
        }
    }

    /// Применяет набор привязок: старые снимаются, новые регистрируются.
    pub fn apply(&mut self, bindings: Vec<(String, Bind)>, pass_through: bool) {
        if bindings == self.bindings && pass_through == self.pass_through {
            return;
        }
        self.bindings = bindings;
        self.pass_through = pass_through;
        if !self.paused {
            self.register_current();
        }
    }

    /// Пока пользователь назначает клавишу, глобальные клавиши отключены,
    /// иначе нажатие перехватится и до окна не дойдёт.
    pub fn set_paused(&mut self, paused: bool) {
        if self.paused == paused {
            return;
        }
        self.paused = paused;
        self.poll.paused.store(paused, Ordering::Relaxed);
        if paused {
            self.unregister_all();
        } else {
            self.register_current();
        }
    }

    fn unregister_all(&mut self) {
        if let Some(m) = &self.manager
            && !self.registered.is_empty()
        {
            let _ = m.unregister_all(&self.registered);
        }
        self.registered.clear();
        self.map.lock().clear();
    }

    fn register_current(&mut self) {
        self.unregister_all();
        self.errors.clear();
        let mut map = HashMap::new();
        let mut polled: Vec<PollEntry> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for (text, bind) in &self.bindings {
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let combo = match KeyCombo::parse(text) {
                Ok(c) => c,
                Err(e) => {
                    self.errors.insert(text.to_string(), e);
                    continue;
                }
            };
            if !seen.insert(combo.display().to_lowercase()) {
                self.errors.insert(text.to_string(), tr("эта комбинация уже назначена другому действию").into());
                continue;
            }
            let poll_it =
                combo.is_mouse() || (self.pass_through && matches!(bind, Bind::Sound(_) | Bind::RandomSet(..)));
            if poll_it {
                if !cfg!(windows) {
                    self.errors
                        .insert(text.to_string(), tr("кнопки мыши и пропуск нажатий работают только в Windows").into());
                    continue;
                }
                match crate::keys::vk_code(&combo.key) {
                    Some(vk) => polled.push(PollEntry { combo, vk, bind: bind.clone() }),
                    None => {
                        self.errors
                            .insert(text.to_string(), tr("эту клавишу нельзя отслеживать в режиме пропуска").into());
                    }
                }
                continue;
            }
            let Some(manager) = &self.manager else { continue };
            let hotkey = match parse_hotkey(text) {
                Ok(h) => h,
                Err(e) => {
                    self.errors.insert(text.to_string(), e);
                    continue;
                }
            };
            match manager.register(hotkey) {
                Ok(()) => {
                    self.registered.push(hotkey);
                    map.insert(hotkey.id(), bind.clone());
                }
                Err(e) => {
                    log::warn!("Не удалось назначить {text}: {e}");
                    self.errors.insert(text.to_string(), tr("занята другой программой").into());
                }
            }
        }
        let n_polled = polled.len();
        *self.map.lock() = map;
        *self.poll.entries.lock() = polled;
        self.poll.version.fetch_add(1, Ordering::Relaxed);
        log::info!("Горячих клавиш: {} штатно, {} опросом", self.registered.len(), n_polled);
    }
}

/// Опрос состояния клавиш и кнопок мыши (Windows).
fn poll_loop(poll: Arc<PollShared>, dispatch: Dispatch) {
    let mut prev: Vec<bool> = Vec::new();
    let mut version = u64::MAX;
    loop {
        let empty = poll.entries.lock().is_empty();
        if empty || poll.paused.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(60));
            prev.clear();
            continue;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        let v = poll.version.load(Ordering::Relaxed);
        let (ctrl, alt, shift, win) = crate::keys::modifiers_down();
        let mut fired: Vec<Bind> = Vec::new();
        {
            let entries = poll.entries.lock();
            if v != version {
                // Новый набор: считаем текущее состояние исходным, чтобы не сработать на зажатой клавише.
                prev = entries.iter().map(|e| crate::keys::is_down(e.vk)).collect();
                version = v;
                continue;
            }
            for (i, e) in entries.iter().enumerate() {
                let c = &e.combo;
                let down =
                    crate::keys::is_down(e.vk) && c.ctrl == ctrl && c.alt == alt && c.shift == shift && c.win == win;
                if down && !prev[i] {
                    fired.push(e.bind.clone());
                }
                prev[i] = down;
            }
        }
        for b in fired {
            dispatch(b);
        }
    }
}

pub fn parse_hotkey(text: &str) -> Result<HotKey, String> {
    HotKey::from_str(text.trim()).map_err(|_| tf!("не понимаю комбинацию «{}»", text))
}

/// Кнопка мыши, нажатая в окне, → строка вида `Ctrl+Mouse4`.
pub fn mouse_from_egui(button: eframe::egui::PointerButton, mods: eframe::egui::Modifiers) -> Option<String> {
    use eframe::egui::PointerButton;
    let name = match button {
        PointerButton::Middle => "MouseMiddle",
        PointerButton::Extra1 => "Mouse4",
        PointerButton::Extra2 => "Mouse5",
        PointerButton::Primary => "MouseLeft",
        PointerButton::Secondary => "MouseRight",
    };
    let mut parts = Vec::new();
    if mods.ctrl || mods.command {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    parts.push(name);
    let s = parts.join("+");
    KeyCombo::parse(&s).ok().map(|_| s)
}

/// Превращает нажатие в окне в строку вида `Ctrl+Alt+K`. `None` — клавиша не подходит.
pub fn hotkey_from_egui(key: eframe::egui::Key, mods: eframe::egui::Modifiers) -> Option<String> {
    use eframe::egui::Key;
    let name: String = match key {
        Key::Num0 => "0".into(),
        Key::Num1 => "1".into(),
        Key::Num2 => "2".into(),
        Key::Num3 => "3".into(),
        Key::Num4 => "4".into(),
        Key::Num5 => "5".into(),
        Key::Num6 => "6".into(),
        Key::Num7 => "7".into(),
        Key::Num8 => "8".into(),
        Key::Num9 => "9".into(),
        Key::ArrowUp => "Up".into(),
        Key::ArrowDown => "Down".into(),
        Key::ArrowLeft => "Left".into(),
        Key::ArrowRight => "Right".into(),
        Key::Space => "Space".into(),
        Key::Enter => "Enter".into(),
        Key::Tab => "Tab".into(),
        Key::Backspace => "Backspace".into(),
        Key::Insert => "Insert".into(),
        Key::Delete => "Delete".into(),
        Key::Home => "Home".into(),
        Key::End => "End".into(),
        Key::PageUp => "PageUp".into(),
        Key::PageDown => "PageDown".into(),
        Key::Minus => "Minus".into(),
        Key::Equals | Key::Plus => "Equal".into(),
        Key::Comma => "Comma".into(),
        Key::Period => "Period".into(),
        Key::Semicolon => "Semicolon".into(),
        Key::Slash => "Slash".into(),
        Key::Backslash => "Backslash".into(),
        Key::OpenBracket => "BracketLeft".into(),
        Key::CloseBracket => "BracketRight".into(),
        Key::Backtick => "Backquote".into(),
        Key::Quote => "Quote".into(),
        other => {
            let n = other.name();
            // Буквы и F1–F24.
            let is_letter = n.len() == 1 && n.chars().all(|c| c.is_ascii_alphabetic());
            let is_f = n.starts_with('F') && n[1..].parse::<u32>().is_ok_and(|x| (1..=24).contains(&x));
            if is_letter || is_f {
                n.to_ascii_uppercase()
            } else {
                return None;
            }
        }
    };
    let mut parts = Vec::new();
    if mods.ctrl || mods.command {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    let mut s = parts.join("+");
    if !s.is_empty() {
        s.push('+');
    }
    s.push_str(&name);
    // Проверяем, что библиотека понимает получившуюся строку.
    parse_hotkey(&s).ok().map(|_| s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Key, Modifiers};

    #[test]
    fn egui_keys_to_strings() {
        assert_eq!(hotkey_from_egui(Key::K, Modifiers::CTRL | Modifiers::ALT).as_deref(), Some("Ctrl+Alt+K"));
        assert_eq!(hotkey_from_egui(Key::F13, Modifiers::NONE).as_deref(), Some("F13"));
        assert_eq!(hotkey_from_egui(Key::Num1, Modifiers::SHIFT).as_deref(), Some("Shift+1"));
        assert!(parse_hotkey("Ctrl+Numpad1").is_ok());
        assert!(parse_hotkey("Ctrl+Alt+").is_err());
        assert_eq!(
            mouse_from_egui(eframe::egui::PointerButton::Extra1, Modifiers::CTRL).as_deref(),
            Some("Ctrl+Mouse4")
        );
        assert_eq!(mouse_from_egui(eframe::egui::PointerButton::Primary, Modifiers::NONE), None);
    }
}
