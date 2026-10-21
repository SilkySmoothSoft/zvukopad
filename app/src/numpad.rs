//! Набор номера звука на нумпаде — как Numcode в Soundpad.
//!
//! Зажимаете модификатор (по умолчанию Alt), набираете цифры на нумпаде, отпускаете —
//! играет звук с этим номером. Пока набираете, оверлей показывает подходящие звуки.
//! * `0` (одна цифра) — повторить последний звук.
//! * Точка на нумпаде (`Del`/`,`) — отменить набор.
//!
//! Цифры регистрируются штатно (RegisterHotKey), отпускание модификатора отслеживается
//! опросом его состояния — перехватчиков клавиатуры нет.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use zv_core::tf;

use parking_lot::{Mutex, RwLock};
use zv_core::settings::NumpadModifier;

use crate::player::Player;

/// Сколько цифр можно набрать.
const MAX_DIGITS: usize = 6;
/// Без Windows (нельзя узнать, отпущен ли модификатор) — срабатывать через эту паузу.
#[cfg(not(windows))]
const RELEASE_TIMEOUT: Duration = Duration::from_millis(1200);
/// Если долго ничего не происходит — сбросить набор.
const IDLE_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Default)]
struct State {
    buffer: String,
    last: Option<Instant>,
}

pub struct Numpad {
    state: Mutex<State>,
    /// Номер → id звука.
    codes: RwLock<HashMap<u32, String>>,
    modifier: Mutex<NumpadModifier>,
    watching: AtomicBool,
    player: Arc<Player>,
    repaint: Box<dyn Fn() + Send + Sync>,
}

impl Numpad {
    pub fn new(player: Arc<Player>, repaint: impl Fn() + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
            codes: RwLock::new(HashMap::new()),
            modifier: Mutex::new(NumpadModifier::Alt),
            watching: AtomicBool::new(false),
            player,
            repaint: Box::new(repaint),
        })
    }

    pub fn set_codes(&self, codes: HashMap<u32, String>) {
        *self.codes.write() = codes;
    }

    pub fn set_modifier(&self, m: NumpadModifier) {
        *self.modifier.lock() = m;
    }

    /// Что сейчас набрано (пусто — ничего не набирается).
    pub fn buffer(&self) -> String {
        self.state.lock().buffer.clone()
    }

    pub fn digit(self: &Arc<Self>, d: u8) {
        {
            let mut s = self.state.lock();
            if s.buffer.len() < MAX_DIGITS {
                s.buffer.push((b'0' + d) as char);
            }
            s.last = Some(Instant::now());
        }
        (self.repaint)();
        if !self.watching.swap(true, Ordering::SeqCst) {
            let me = self.clone();
            std::thread::Builder::new()
                .name("zv-numpad".into())
                .spawn(move || me.watch())
                .expect("не удалось запустить поток нумпада");
        }
    }

    pub fn cancel(&self) {
        self.state.lock().buffer.clear();
        (self.repaint)();
    }

    fn modifier_held(&self) -> bool {
        #[cfg(windows)]
        {
            let (ctrl, alt, shift, _) = crate::keys::modifiers_down();
            match *self.modifier.lock() {
                NumpadModifier::Alt => alt,
                NumpadModifier::Ctrl => ctrl,
                NumpadModifier::Shift => shift,
                NumpadModifier::CtrlAlt => ctrl && alt,
            }
        }
        #[cfg(not(windows))]
        {
            self.state.lock().last.is_some_and(|t| t.elapsed() < RELEASE_TIMEOUT)
        }
    }

    fn watch(&self) {
        loop {
            std::thread::sleep(Duration::from_millis(15));
            let idle = self.state.lock().last.is_none_or(|t| t.elapsed() > IDLE_TIMEOUT);
            if self.state.lock().buffer.is_empty() || idle {
                self.state.lock().buffer.clear();
                break;
            }
            if !self.modifier_held() {
                let code = std::mem::take(&mut self.state.lock().buffer);
                self.commit(&code);
                break;
            }
        }
        self.watching.store(false, Ordering::SeqCst);
        (self.repaint)();
    }

    fn commit(&self, code: &str) {
        if code == "0" {
            self.player.repeat_last();
            return;
        }
        let Ok(n) = code.parse::<u32>() else { return };
        let id = self.codes.read().get(&n).cloned();
        match id {
            Some(id) => self.player.play(&id, None),
            None => self.player.message(tf!("Нет звука с номером {}", n)),
        }
    }

    /// Подсказки: звуки, чей номер начинается с набранного. Сначала точное совпадение,
    /// потом короткие номера, потом по возрастанию.
    pub fn hints(&self, prefix: &str, limit: usize) -> (Vec<(u32, String)>, usize) {
        let codes = self.codes.read();
        let mut v: Vec<(u32, String)> =
            codes.iter().filter(|(c, _)| c.to_string().starts_with(prefix)).map(|(c, id)| (*c, id.clone())).collect();
        v.sort_by_key(|(c, _)| (c.to_string().len(), *c));
        let total = v.len();
        v.truncate(limit);
        (v, total)
    }
}
