//! Очередь: звуки играют по очереди, с паузой между ними, по кругу или вперемешку.
//!
//! Работает в отдельном потоке, поэтому не зависит от того, свёрнуто ли окно.

use std::sync::Arc;
use std::time::{Duration, Instant};
use zv_core::i18n::tr;

use parking_lot::Mutex;

use crate::player::Player;

#[derive(Debug, Clone)]
pub struct QueueState {
    pub items: Vec<String>,
    /// Какой элемент играет (или будет играть следующим).
    pub current: Option<usize>,
    pub playing: bool,
    pub repeat: bool,
    pub shuffle: bool,
    /// Пауза между звуками, секунды.
    pub gap: f32,
    phase: Phase,
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    Idle,
    /// Звук запущен, ждём, пока он загрузится и зазвучит.
    Starting(Instant),
    Playing,
    /// Пауза между звуками.
    Gap(Instant),
}

impl Default for QueueState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            current: None,
            playing: false,
            repeat: false,
            shuffle: false,
            gap: 0.3,
            phase: Phase::Idle,
        }
    }
}

pub struct Queue {
    pub state: Mutex<QueueState>,
    player: Arc<Player>,
    repaint: Box<dyn Fn() + Send + Sync>,
}

impl Queue {
    pub fn new(player: Arc<Player>, repaint: impl Fn() + Send + Sync + 'static) -> Arc<Self> {
        let q = Arc::new(Self { state: Mutex::new(QueueState::default()), player, repaint: Box::new(repaint) });
        let weak = Arc::downgrade(&q);
        std::thread::Builder::new()
            .name("zv-queue".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_millis(30));
                    let Some(q) = weak.upgrade() else { break };
                    q.tick();
                }
            })
            .expect("не удалось запустить поток очереди");
        q
    }

    pub fn snapshot(&self) -> QueueState {
        self.state.lock().clone()
    }

    pub fn len(&self) -> usize {
        self.state.lock().items.len()
    }

    pub fn add(&self, id: &str) {
        self.state.lock().items.push(id.to_string());
        (self.repaint)();
    }

    pub fn remove(&self, index: usize) {
        let mut s = self.state.lock();
        if index >= s.items.len() {
            return;
        }
        let removed_current = s.current == Some(index);
        let removed_id = s.items.remove(index);
        if let Some(c) = s.current {
            if c > index {
                s.current = Some(c - 1);
            } else if removed_current {
                s.current = if s.items.is_empty() { None } else { Some(c.min(s.items.len() - 1)) };
                if s.playing {
                    // Удалили то, что играет — останавливаем и переходим к следующему.
                    s.phase = Phase::Idle;
                    drop(s);
                    self.player.stop(&removed_id);
                }
            }
        }
    }

    pub fn move_item(&self, index: usize, up: bool) {
        let mut s = self.state.lock();
        let other = if up { index.checked_sub(1) } else { Some(index + 1) };
        let Some(other) = other.filter(|&o| o < s.items.len()) else { return };
        s.items.swap(index, other);
        if s.current == Some(index) {
            s.current = Some(other);
        } else if s.current == Some(other) {
            s.current = Some(index);
        }
    }

    pub fn clear(&self) {
        self.stop();
        let mut s = self.state.lock();
        s.items.clear();
        s.current = None;
    }

    pub fn set_options(&self, repeat: bool, shuffle: bool, gap: f32) {
        let mut s = self.state.lock();
        s.repeat = repeat;
        s.shuffle = shuffle;
        s.gap = gap.clamp(0.0, 60.0);
    }

    pub fn play(&self) {
        let mut s = self.state.lock();
        if s.items.is_empty() {
            drop(s);
            self.player.message(tr("Очередь пуста — добавьте звуки (правый клик → «В очередь»)").into());
            return;
        }
        s.playing = true;
        if s.current.is_none_or(|c| c >= s.items.len()) {
            s.current = Some(0);
        }
        s.phase = Phase::Idle;
        drop(s);
        (self.repaint)();
    }

    /// Начать с конкретного элемента.
    pub fn play_from(&self, index: usize) {
        if let Some(id) = self.current_id() {
            self.player.stop(&id);
        }
        let mut s = self.state.lock();
        if index < s.items.len() {
            s.current = Some(index);
            s.playing = true;
            s.phase = Phase::Idle;
        }
    }

    pub fn stop(&self) {
        let id = self.current_id();
        let mut s = self.state.lock();
        let was = s.playing;
        s.playing = false;
        s.phase = Phase::Idle;
        drop(s);
        if was && let Some(id) = id {
            self.player.stop(&id);
        }
        (self.repaint)();
    }

    pub fn toggle(&self) {
        if self.state.lock().playing { self.stop() } else { self.play() }
    }

    /// Сразу к следующему звуку.
    pub fn next(&self) {
        if let Some(id) = self.current_id() {
            self.player.stop(&id);
        }
        let mut s = self.state.lock();
        if !s.playing {
            drop(s);
            self.play();
            return;
        }
        advance(&mut s);
        s.phase = Phase::Idle;
    }

    fn current_id(&self) -> Option<String> {
        let s = self.state.lock();
        s.current.and_then(|c| s.items.get(c).cloned())
    }

    fn tick(&self) {
        let mut s = self.state.lock();
        if !s.playing {
            return;
        }
        let Some(cur) = s.current.filter(|&c| c < s.items.len()) else {
            s.playing = false;
            drop(s);
            (self.repaint)();
            return;
        };
        let id = s.items[cur].clone();
        match s.phase {
            Phase::Idle => {
                s.phase = Phase::Starting(Instant::now());
                drop(s);
                if self.player.has(&id) {
                    self.player.play(&id, None);
                } else {
                    // Звук удалили из библиотеки — пропускаем.
                    let mut s = self.state.lock();
                    advance(&mut s);
                    s.phase = Phase::Idle;
                }
                (self.repaint)();
            }
            Phase::Starting(t) => {
                if self.player.is_sound_playing(&id) {
                    s.phase = Phase::Playing;
                } else if t.elapsed() > Duration::from_secs(10) {
                    log::warn!("Очередь: «{id}» не зазвучал, пропускаю");
                    advance(&mut s);
                    s.phase = Phase::Idle;
                }
            }
            Phase::Playing => {
                if !self.player.is_sound_playing(&id) {
                    s.phase = Phase::Gap(Instant::now());
                }
            }
            Phase::Gap(t) => {
                if t.elapsed().as_secs_f32() >= s.gap {
                    let was_last = cur + 1 >= s.items.len();
                    if was_last && !s.repeat && !s.shuffle {
                        s.playing = false;
                        s.current = Some(0);
                        s.phase = Phase::Idle;
                        drop(s);
                        (self.repaint)();
                        return;
                    }
                    advance(&mut s);
                    s.phase = Phase::Idle;
                }
            }
        }
    }
}

fn advance(s: &mut QueueState) {
    let n = s.items.len();
    if n == 0 {
        s.current = None;
        return;
    }
    let cur = s.current.unwrap_or(0);
    s.current = Some(if s.shuffle && n > 1 {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as usize)
            .unwrap_or(1);
        let mut next = seed % n;
        if next == cur {
            next = (next + 1) % n;
        }
        next
    } else {
        (cur + 1) % n
    });
}
