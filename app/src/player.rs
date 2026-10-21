//! Проигрыватель: связывает библиотеку звуков со звуковым движком.
//!
//! Им пользуются и интерфейс, и горячие клавиши (из другого места программы), поэтому
//! всё внутри защищено блокировками, а сам объект живёт в `Arc`.
//! Декодирование файлов идёт в фоновом потоке, чтобы не подвешивать окно.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use zv_core::i18n::tr;
use zv_core::tf;

use crossbeam_channel::{Receiver, Sender};
use parking_lot::{Mutex, RwLock};
use zv_audio::{Engine, SoundData};
use zv_core::settings::{PlayMode, PlayTarget};

/// Сколько памяти можно занять декодированными звуками.
const CACHE_LIMIT_BYTES: usize = 768 * 1024 * 1024;
/// Файлы меньше этого размера загружаются заранее, чтобы звучать мгновенно.
const PRELOAD_MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct SoundSlot {
    pub path: PathBuf,
    pub volume: f32,
    pub has_hotkey: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayConfig {
    pub target: PlayTarget,
    pub mode: PlayMode,
    pub normalize: bool,
}

struct CacheEntry {
    data: Arc<SoundData>,
    last_used: Instant,
}

struct LoadJob {
    id: String,
    path: PathBuf,
    /// Сыграть сразу после загрузки (звук вызвали до того, как он загрузился).
    play: Option<PlayTarget>,
}

pub struct Player {
    engine: Engine,
    slots: RwLock<HashMap<String, SoundSlot>>,
    cache: Mutex<HashMap<String, CacheEntry>>,
    loading: Mutex<HashSet<String>>,
    durations: RwLock<HashMap<String, f64>>,
    config: RwLock<PlayConfig>,
    load_tx: Sender<LoadJob>,
    messages: Mutex<Vec<String>>,
    repaint: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    /// Последний сыгранный звук библиотеки (для «повторить последний»).
    last_played: Mutex<Option<String>>,
    /// Сюда сообщается о каждом запуске звука библиотеки (для оверлея).
    played_tx: Mutex<Option<Sender<String>>>,
}

/// Постоянный числовой ключ звука для движка.
pub fn sound_key(id: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    h.finish()
}

impl Player {
    pub fn new(engine: Engine, config: PlayConfig) -> Arc<Self> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let player = Arc::new(Self {
            engine,
            slots: RwLock::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
            loading: Mutex::new(HashSet::new()),
            durations: RwLock::new(HashMap::new()),
            config: RwLock::new(config),
            load_tx: tx,
            messages: Mutex::new(Vec::new()),
            repaint: Mutex::new(None),
            last_played: Mutex::new(None),
            played_tx: Mutex::new(None),
        });
        // Два потока загрузки: длинный файл не задерживает короткие.
        for i in 0..2 {
            let weak = Arc::downgrade(&player);
            let rx: Receiver<LoadJob> = rx.clone();
            std::thread::Builder::new()
                .name(format!("zv-loader-{i}"))
                .spawn(move || {
                    while let Ok(job) = rx.recv() {
                        let Some(p) = weak.upgrade() else { break };
                        p.load_now(job);
                    }
                })
                .expect("не удалось запустить поток загрузки");
        }
        player
    }

    /// Чем перерисовывать окно, когда что-то поменялось в фоне.
    pub fn set_repaint(&self, f: impl Fn() + Send + Sync + 'static) {
        *self.repaint.lock() = Some(Box::new(f));
    }

    fn request_repaint(&self) {
        if let Some(f) = self.repaint.lock().as_ref() {
            f();
        }
    }

    pub fn set_config(&self, config: PlayConfig) {
        *self.config.write() = config;
    }

    pub fn config(&self) -> PlayConfig {
        *self.config.read()
    }

    /// Новый список звуков после сканирования папки. Загружает заранее нужные звуки.
    pub fn set_slots(&self, slots: HashMap<String, SoundSlot>) {
        // Звуки, которых больше нет или у которых сменился файл, выбрасываем из памяти.
        {
            let mut cache = self.cache.lock();
            cache.retain(|id, _| slots.contains_key(id));
        }
        let to_preload: Vec<(String, PathBuf)> = slots
            .iter()
            .filter(|(id, slot)| {
                !self.cache.lock().contains_key(*id)
                    && (slot.has_hotkey
                        || std::fs::metadata(&slot.path).map(|m| m.len() <= PRELOAD_MAX_FILE_BYTES).unwrap_or(false))
            })
            .map(|(id, slot)| (id.clone(), slot.path.clone()))
            .collect();
        *self.slots.write() = slots;
        for (id, path) in to_preload {
            self.enqueue(id, path, None);
        }
    }

    /// Обновляет громкость звука без пересканирования.
    pub fn update_slot(&self, id: &str, volume: f32, has_hotkey: bool) {
        if let Some(s) = self.slots.write().get_mut(id) {
            s.volume = volume;
            s.has_hotkey = has_hotkey;
        }
    }

    /// Файл изменился или удалён — забыть загруженные данные.
    pub fn forget(&self, id: &str) {
        self.cache.lock().remove(id);
        self.durations.write().remove(id);
    }

    pub fn duration(&self, id: &str) -> Option<f64> {
        self.durations.read().get(id).copied()
    }

    pub fn is_loading(&self, id: &str) -> bool {
        self.loading.lock().contains(id)
    }

    fn enqueue(&self, id: String, path: PathBuf, play: Option<PlayTarget>) {
        if !self.loading.lock().insert(id.clone()) && play.is_none() {
            return; // уже грузится
        }
        let _ = self.load_tx.send(LoadJob { id, path, play });
    }

    fn load_now(&self, job: LoadJob) {
        // Мог загрузиться, пока задание ждало очереди.
        let cached = self.cache.lock().get(&job.id).map(|e| e.data.clone());
        let result = match cached {
            Some(d) => Ok(d),
            None => zv_audio::decode_file(&job.path).map(Arc::new),
        };
        self.loading.lock().remove(&job.id);
        match result {
            Ok(data) => {
                self.durations.write().insert(job.id.clone(), data.duration_secs());
                self.insert_cache(&job.id, data.clone());
                if let Some(target) = job.play {
                    self.play_data(&job.id, data, target);
                }
            }
            Err(e) => {
                let name = job.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                log::error!("Не удалось загрузить {}: {e}", job.path.display());
                self.message(format!("«{name}»: {e}"));
            }
        }
        self.request_repaint();
    }

    fn insert_cache(&self, id: &str, data: Arc<SoundData>) {
        let mut cache = self.cache.lock();
        cache.insert(id.to_string(), CacheEntry { data, last_used: Instant::now() });
        let mut total: usize = cache.values().map(|e| e.data.memory_bytes()).sum();
        if total <= CACHE_LIMIT_BYTES {
            return;
        }
        // Вытесняем давно не игравшие звуки, но не те, что на горячих клавишах.
        let slots = self.slots.read();
        let mut candidates: Vec<(String, Instant, usize)> = cache
            .iter()
            .filter(|(k, _)| k.as_str() != id && !slots.get(k.as_str()).is_some_and(|s| s.has_hotkey))
            .map(|(k, e)| (k.clone(), e.last_used, e.data.memory_bytes()))
            .collect();
        candidates.sort_by_key(|c| c.1);
        for (k, _, size) in candidates {
            if total <= CACHE_LIMIT_BYTES {
                break;
            }
            cache.remove(&k);
            total -= size;
        }
    }

    /// Куда сообщать о запуске звуков.
    pub fn set_played_listener(&self, tx: Sender<String>) {
        *self.played_tx.lock() = Some(tx);
    }

    /// Есть ли такой звук в библиотеке.
    pub fn has(&self, id: &str) -> bool {
        self.slots.read().contains_key(id)
    }

    /// Звучит ли сейчас звук библиотеки.
    pub fn is_sound_playing(&self, id: &str) -> bool {
        self.engine.poll();
        self.engine.is_playing(sound_key(id))
    }

    pub fn last_played(&self) -> Option<String> {
        self.last_played.lock().clone()
    }

    /// Повторить последний сыгранный звук.
    pub fn repeat_last(&self) {
        match self.last_played() {
            Some(id) => self.play(&id, None),
            None => self.message(tr("Ещё ничего не играло — повторять нечего").into()),
        }
    }

    /// Случайный звук из библиотеки (не тот же, что только что играл).
    pub fn play_random(&self) {
        let ids: Vec<String> = self.slots.read().keys().cloned().collect();
        self.play_random_from(&ids, tr("Звуков нет"));
    }

    /// Случайный звук из набора (например, из тега). Не повторяет только что игравший.
    pub fn play_random_from(&self, ids: &[String], empty_msg: &str) {
        let ids: Vec<&String> = {
            let slots = self.slots.read();
            ids.iter().filter(|id| slots.contains_key(*id)).collect()
        };
        if ids.is_empty() {
            self.message(empty_msg.to_string());
            return;
        }
        let last = self.last_played();
        let candidates: Vec<&String> =
            ids.iter().copied().filter(|id| ids.len() == 1 || Some(*id) != last.as_ref()).collect();
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.subsec_nanos() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ d.as_secs())
            .unwrap_or(0);
        let id = candidates[(seed >> 17) as usize % candidates.len()].clone();
        self.play(&id, None);
    }

    /// Пауза/продолжить. Возвращает новое состояние (true — пауза).
    pub fn toggle_pause(&self) -> bool {
        let p = !self.engine.is_paused();
        self.engine.set_paused(p);
        self.request_repaint();
        p
    }

    /// Играет звук из библиотеки. `target = None` — куда выбрано в настройках.
    pub fn play(&self, id: &str, target: Option<PlayTarget>) {
        if self.has(id) {
            *self.last_played.lock() = Some(id.to_string());
            if let Some(tx) = self.played_tx.lock().as_ref() {
                let _ = tx.try_send(id.to_string());
            }
        }
        let target = target.unwrap_or(self.config().target);
        let cached = {
            let mut cache = self.cache.lock();
            cache.get_mut(id).map(|e| {
                e.last_used = Instant::now();
                e.data.clone()
            })
        };
        match cached {
            Some(data) => self.play_data(id, data, target),
            None => {
                let path = self.slots.read().get(id).map(|s| s.path.clone());
                match path {
                    Some(path) => self.enqueue(id.to_string(), path, Some(target)),
                    None => self.message(tf!("Звук «{}» не найден", id)),
                }
            }
        }
    }

    fn play_data(&self, id: &str, data: Arc<SoundData>, target: PlayTarget) {
        let cfg = self.config();
        let volume = self.slots.read().get(id).map(|s| s.volume).unwrap_or(1.0);
        let gain = volume * if cfg.normalize { data.norm_gain } else { 1.0 };
        if let Err(e) = self.engine.play(sound_key(id), data, gain, target, cfg.mode) {
            self.message(e);
        }
        self.request_repaint();
    }

    /// Играет звук, которого нет в библиотеке (например, только что синтезированную речь).
    pub fn play_adhoc(&self, key_name: &str, data: Arc<SoundData>, target: PlayTarget) {
        let cfg = self.config();
        let gain = if cfg.normalize { data.norm_gain } else { 1.0 };
        if let Err(e) = self.engine.play(sound_key(key_name), data, gain, target, PlayMode::Restart) {
            self.message(e);
        }
        self.request_repaint();
    }

    /// Играет готовый звук (например, паттерн редактора), по кругу или один раз.
    pub fn play_pattern(&self, key_name: &str, data: Arc<SoundData>, target: PlayTarget, looping: bool) {
        if let Err(e) = self.engine.play_ex(sound_key(key_name), data, 1.0, target, PlayMode::Restart, looping) {
            self.message(e);
        }
        self.request_repaint();
    }

    /// Загрузить звук в память заранее (без воспроизведения) — например, чтобы показать волну.
    pub fn preload(&self, id: &str) {
        if self.cache.lock().contains_key(id) {
            return;
        }
        let path = self.slots.read().get(id).map(|s| s.path.clone());
        if let Some(path) = path {
            self.enqueue(id.to_string(), path, None);
        }
    }

    /// Загруженные данные звука (для волны), если он уже в памяти.
    pub fn cached_data(&self, id: &str) -> Option<Arc<SoundData>> {
        self.cache.lock().get(id).map(|e| e.data.clone())
    }

    /// Где играет звук: (секунды, на паузе). `None` — не играет.
    pub fn position(&self, id: &str) -> Option<(f64, bool)> {
        self.engine.position(sound_key(id))
    }

    /// Пауза/продолжение одного звука. Возвращает новое состояние (true — пауза).
    pub fn toggle_sound_pause(&self, id: &str) -> bool {
        let paused = !self.position(id).is_some_and(|(_, p)| p);
        self.engine.pause_key(sound_key(id), paused);
        self.request_repaint();
        paused
    }

    pub fn seek(&self, id: &str, secs: f64) {
        self.engine.seek_key(sound_key(id), secs);
    }

    pub fn stop(&self, id: &str) {
        self.engine.stop_key(sound_key(id));
    }

    pub fn stop_all(&self) {
        self.engine.stop_all();
    }

    pub fn message(&self, text: String) {
        self.messages.lock().push(text);
        self.request_repaint();
    }

    /// Сообщения об ошибках для показа в интерфейсе.
    pub fn take_messages(&self) -> Vec<String> {
        std::mem::take(&mut *self.messages.lock())
    }
}
