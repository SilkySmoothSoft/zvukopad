//! Разбор строк клавиш: `Ctrl+Alt+K`, `F13`, `Numpad1`, `Mouse4`, `Ctrl+MouseMiddle`.
//!
//! Клавиатурные сочетания регистрируются штатно (RegisterHotKey) через global-hotkey.
//! Кнопки мыши и режим «пропускать нажатия в игру» работают опросом состояния клавиш
//! (GetAsyncKeyState) — без перехватчиков клавиатуры и без подмены ввода.

use std::str::FromStr;
use zv_core::i18n::tr;
use zv_core::tf;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    /// Имя клавиши в верхнем регистре: `K`, `F13`, `NUMPAD1`, `MOUSE4`.
    pub key: String,
}

const MOUSE_KEYS: &[&str] = &["MOUSELEFT", "MOUSERIGHT", "MOUSEMIDDLE", "MOUSE4", "MOUSE5"];

impl KeyCombo {
    pub fn parse(text: &str) -> Result<Self, String> {
        let err = || tf!("не понимаю сочетание «{}»", text);
        let mut c = KeyCombo { ctrl: false, alt: false, shift: false, win: false, key: String::new() };
        let tokens: Vec<&str> = text.split('+').map(str::trim).collect();
        if tokens.iter().any(|t| t.is_empty()) {
            return Err(err());
        }
        let (last, mods) = tokens.split_last().ok_or_else(err)?;
        for m in mods {
            match m.to_ascii_uppercase().as_str() {
                "CTRL" | "CONTROL" => c.ctrl = true,
                "ALT" | "OPTION" => c.alt = true,
                "SHIFT" => c.shift = true,
                "WIN" | "SUPER" | "META" | "CMD" => c.win = true,
                _ => return Err(err()),
            }
        }
        let key = last.to_ascii_uppercase();
        c.key = match key.as_str() {
            "MOUSE3" | "MMB" | "MIDDLEMOUSE" => "MOUSEMIDDLE".into(),
            "XBUTTON1" | "MOUSEBACK" => "MOUSE4".into(),
            "XBUTTON2" | "MOUSEFORWARD" => "MOUSE5".into(),
            "LMB" | "MOUSE1" => "MOUSELEFT".into(),
            "RMB" | "MOUSE2" => "MOUSERIGHT".into(),
            _ => key,
        };
        if c.is_mouse() {
            if matches!(c.key.as_str(), "MOUSELEFT" | "MOUSERIGHT") && !c.has_modifier() {
                return Err(tr("левую и правую кнопки мыши можно назначать только с Ctrl, Alt или Shift").into());
            }
        } else if global_hotkey::hotkey::HotKey::from_str(text).is_err() {
            return Err(err());
        }
        Ok(c)
    }

    pub fn is_mouse(&self) -> bool {
        MOUSE_KEYS.contains(&self.key.as_str())
    }

    pub fn has_modifier(&self) -> bool {
        self.ctrl || self.alt || self.shift || self.win
    }

    /// Красивое имя для показа: `Ctrl+Alt+K`, `Mouse4`, `Колесо (средняя)`.
    pub fn display(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if self.shift {
            parts.push("Shift".into());
        }
        if self.win {
            parts.push("Win".into());
        }
        let key = match self.key.as_str() {
            "MOUSELEFT" => "MouseLeft".to_string(),
            "MOUSERIGHT" => "MouseRight".to_string(),
            "MOUSEMIDDLE" => "MouseMiddle".to_string(),
            "MOUSE4" => "Mouse4".to_string(),
            "MOUSE5" => "Mouse5".to_string(),
            k if k.starts_with("NUMPAD") => format!("Numpad{}", k[6..].to_lowercase()),
            k if k.len() == 1 => k.to_string(),
            k => {
                let mut s = k.to_lowercase();
                if let Some(f) = s.get_mut(0..1) {
                    f.make_ascii_uppercase();
                }
                s
            }
        };
        parts.push(key);
        parts.join("+")
    }
}

/// Проверяет строку клавиши (клавиатура или мышь).
pub fn validate(text: &str) -> Result<(), String> {
    KeyCombo::parse(text).map(|_| ())
}

/// Код виртуальной клавиши Windows для имени клавиши.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn vk_code(key: &str) -> Option<i32> {
    let k = key.to_ascii_uppercase();
    let b = k.as_bytes();
    if b.len() == 1 {
        let c = b[0];
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as i32);
        }
    }
    if let Some(n) = k.strip_prefix("DIGIT").and_then(|d| d.parse::<i32>().ok()) {
        return Some(0x30 + n);
    }
    if let Some(n) = k.strip_prefix("KEY") {
        return vk_code(n);
    }
    if let Some(n) = k.strip_prefix('F').and_then(|d| d.parse::<i32>().ok())
        && (1..=24).contains(&n)
    {
        return Some(0x6F + n);
    }
    if let Some(rest) = k.strip_prefix("NUMPAD").or_else(|| k.strip_prefix("NUM")) {
        if let Ok(n) = rest.parse::<i32>()
            && (0..=9).contains(&n)
        {
            return Some(0x60 + n);
        }
        return match rest {
            "MULTIPLY" | "STAR" => Some(0x6A),
            "ADD" | "PLUS" => Some(0x6B),
            "SUBTRACT" => Some(0x6D),
            "DECIMAL" => Some(0x6E),
            "DIVIDE" => Some(0x6F),
            "ENTER" => Some(0x0D),
            _ => None,
        };
    }
    Some(match k.as_str() {
        "MOUSELEFT" => 0x01,
        "MOUSERIGHT" => 0x02,
        "MOUSEMIDDLE" => 0x04,
        "MOUSE4" => 0x05,
        "MOUSE5" => 0x06,
        "BACKSPACE" => 0x08,
        "TAB" => 0x09,
        "ENTER" => 0x0D,
        "PAUSE" | "PAUSEBREAK" => 0x13,
        "CAPSLOCK" => 0x14,
        "ESCAPE" | "ESC" => 0x1B,
        "SPACE" => 0x20,
        "PAGEUP" => 0x21,
        "PAGEDOWN" => 0x22,
        "END" => 0x23,
        "HOME" => 0x24,
        "LEFT" | "ARROWLEFT" => 0x25,
        "UP" | "ARROWUP" => 0x26,
        "RIGHT" | "ARROWRIGHT" => 0x27,
        "DOWN" | "ARROWDOWN" => 0x28,
        "PRINTSCREEN" => 0x2C,
        "INSERT" => 0x2D,
        "DELETE" => 0x2E,
        "SCROLLLOCK" => 0x91,
        "SEMICOLON" | ";" => 0xBA,
        "EQUAL" | "=" => 0xBB,
        "COMMA" | "," => 0xBC,
        "MINUS" | "-" => 0xBD,
        "PERIOD" | "." => 0xBE,
        "SLASH" | "/" => 0xBF,
        "BACKQUOTE" | "`" => 0xC0,
        "BRACKETLEFT" | "[" => 0xDB,
        "BACKSLASH" | "\\" => 0xDC,
        "BRACKETRIGHT" | "]" => 0xDD,
        "QUOTE" | "'" => 0xDE,
        _ => return None,
    })
}

/// Нажата ли сейчас клавиша (Windows). Вне Windows всегда `false`.
#[cfg(windows)]
pub fn is_down(vk: i32) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
}

#[cfg(not(windows))]
pub fn is_down(_vk: i32) -> bool {
    false
}

/// Состояние модификаторов сейчас: (ctrl, alt, shift, win).
pub fn modifiers_down() -> (bool, bool, bool, bool) {
    (is_down(0x11), is_down(0x12), is_down(0x10), is_down(0x5B) || is_down(0x5C))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display() {
        let c = KeyCombo::parse("ctrl+alt+k").unwrap();
        assert!(c.ctrl && c.alt && !c.shift);
        assert_eq!(c.display(), "Ctrl+Alt+K");
        let m = KeyCombo::parse("Shift+Mouse4").unwrap();
        assert!(m.is_mouse());
        assert_eq!(m.display(), "Shift+Mouse4");
        assert!(KeyCombo::parse("MouseLeft").is_err());
        assert!(KeyCombo::parse("Ctrl+MouseLeft").is_ok());
        assert!(KeyCombo::parse("Ctrl+").is_err());
        assert!(KeyCombo::parse("Ctrl+Numpad3").is_ok());
        assert_eq!(KeyCombo::parse("Ctrl+Numpad3").unwrap().display(), "Ctrl+Numpad3");
        assert_eq!(KeyCombo::parse("F13").unwrap().display(), "F13");
    }

    #[test]
    fn vk_codes() {
        assert_eq!(vk_code("K"), Some(0x4B));
        assert_eq!(vk_code("F13"), Some(0x7C));
        assert_eq!(vk_code("NUMPAD3"), Some(0x63));
        assert_eq!(vk_code("MOUSE5"), Some(0x06));
        assert_eq!(vk_code("UP"), Some(0x26));
        assert_eq!(vk_code("Space"), Some(0x20));
        assert_eq!(vk_code("blah"), None);
    }
}
