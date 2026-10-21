//! Звуковой движок: управляет устройствами и раздаёт команды микшерам.
//!
//! Устройства и потоки живут в отдельном управляющем потоке. Интерфейс общается с движком
//! через [`Engine`] — его можно клонировать и использовать из любого потока.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use zv_core::i18n::tr;
use zv_core::tf;

use cpal::traits::{DeviceTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender};
use parking_lot::{Mutex, RwLock};
use zv_core::settings::{DeviceRef, PlayMode, PlayTarget};

use crate::devices::{Direction, device_name, find_device};
use crate::effects::{BusKind, EffectsParams};
use crate::mixer::{AtomicF32, BusCmd, BusEvent, BusRenderer, BusShared, MicReader, Voice};
use crate::recorder::{RecShared, Recorder};
use crate::replay::{Replay, ReplayShared};
use crate::sound::SoundData;
use crate::streams::{ErrFn, build_input, build_output};

/// Какие устройства использовать.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EngineConfig {
    /// Выход «себе». `None` — устройство Windows по умолчанию.
    pub monitor: Option<DeviceRef>,
    /// Виртуальный выход. `None` — выключен.
    pub virtual_out: Option<DeviceRef>,
    /// Микрофон для подмешивания в виртуальный выход. `None` — микрофон по умолчанию.
    pub mic: Option<DeviceRef>,
    pub mic_enabled: bool,
}

/// Состояние одного потока для показа в интерфейсе.
#[derive(Debug, Clone, Default)]
pub struct StreamStatus {
    pub active: bool,
    pub device: String,
    pub rate: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecordingStatus {
    pub path: PathBuf,
    pub device: String,
    pub started: Instant,
}

#[derive(Debug, Clone, Default)]
pub struct EngineStatus {
    pub monitor: StreamStatus,
    pub virtual_out: StreamStatus,
    pub mic: StreamStatus,
    pub recording: Option<RecordingStatus>,
}

/// Состояние буфера «что сейчас было».
#[derive(Debug, Clone)]
pub struct ReplayInfo {
    pub device: String,
    pub buffered_secs: f64,
    pub capacity_secs: u32,
    pub rate: u32,
}

/// Чем закончилась запись.
#[derive(Debug, Clone)]
pub enum RecordingResult {
    Saved(PathBuf),
    Failed(String),
}

enum Ctl {
    Configure(EngineConfig),
    StreamFailed { which: Which, message: String },
    StartRecording { source: Option<DeviceRef>, loopback: bool, path: PathBuf },
    StopRecording,
    StartReplay { source: Option<DeviceRef>, loopback: bool, secs: u32 },
    StopReplay,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Which {
    Monitor,
    Virtual,
    Mic,
    Recorder,
    Replay,
}

struct BusHandle {
    cmd: Sender<BusCmd>,
    events: Receiver<BusEvent>,
}

#[derive(Default)]
struct Buses {
    monitor: Option<BusHandle>,
    virtual_out: Option<BusHandle>,
}

/// Что сейчас играет: экземпляр → (ключ звука, на скольких выходах ещё звучит).
#[derive(Default)]
struct PlayState {
    instances: HashMap<u64, (u64, u32)>,
    /// Ключ звука → (экземпляр, позиция в секундах, на паузе).
    positions: HashMap<u64, (u64, f64, bool)>,
}

struct Inner {
    ctl: Mutex<mpsc::Sender<Ctl>>,
    buses: RwLock<Buses>,
    status: Mutex<EngineStatus>,
    playing: Mutex<PlayState>,
    monitor_shared: Arc<BusShared>,
    virtual_shared: Arc<BusShared>,
    rec_shared: Arc<RecShared>,
    rec_results: Mutex<Vec<RecordingResult>>,
    next_instance: AtomicU64,
    effects: Arc<EffectsParams>,
    replay: Mutex<Option<(Arc<ReplayShared>, String)>>,
    replay_error: Mutex<Option<String>>,
}

/// Ручка звукового движка. Дешёвая в клонировании.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

impl Engine {
    pub fn start(config: EngineConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = |v: f32| {
            Arc::new(BusShared { volume: AtomicF32::new(v), mic_volume: AtomicF32::new(1.0), ..Default::default() })
        };
        let inner = Arc::new(Inner {
            ctl: Mutex::new(tx.clone()),
            buses: RwLock::new(Buses::default()),
            status: Mutex::new(EngineStatus::default()),
            playing: Mutex::new(PlayState::default()),
            monitor_shared: shared(0.8),
            virtual_shared: shared(1.0),
            rec_shared: Arc::new(RecShared::default()),
            rec_results: Mutex::new(Vec::new()),
            next_instance: AtomicU64::new(1),
            effects: Arc::new(EffectsParams::default()),
            replay: Mutex::new(None),
            replay_error: Mutex::new(None),
        });
        let worker_inner = inner.clone();
        std::thread::Builder::new()
            .name("zv-audio-control".into())
            .spawn(move || control_loop(worker_inner, rx, tx))
            .expect("не удалось запустить поток звука");
        let engine = Self { inner };
        engine.configure(config);
        engine
    }

    fn send(&self, c: Ctl) {
        let _ = self.inner.ctl.lock().send(c);
    }

    /// Применяет набор устройств (пересоздаёт потоки).
    pub fn configure(&self, config: EngineConfig) {
        self.send(Ctl::Configure(config));
    }

    pub fn status(&self) -> EngineStatus {
        self.inner.status.lock().clone()
    }

    // ---------- Громкость и уровни ----------

    pub fn set_monitor_volume(&self, v: f32) {
        self.inner.monitor_shared.volume.set(v);
    }
    pub fn set_virtual_volume(&self, v: f32) {
        self.inner.virtual_shared.volume.set(v);
    }
    pub fn set_mic_volume(&self, v: f32) {
        self.inner.virtual_shared.mic_volume.set(v);
    }

    /// Пиковые уровни с прошлого вызова: (себе, в микрофон, микрофон, запись).
    pub fn take_levels(&self) -> Levels {
        Levels {
            monitor: self.inner.monitor_shared.peak.take(),
            virtual_out: self.inner.virtual_shared.peak.take(),
            mic: self.inner.virtual_shared.mic_peak.take(),
            recording: self.inner.rec_shared.peak.take(),
        }
    }

    // ---------- Эффекты и пауза ----------

    pub fn set_effects(&self, s: &zv_core::effects::EffectsSettings) {
        self.inner.effects.apply(s);
    }

    /// Приглушать звуки, пока вы говорите в микрофон (работает при подмешивании голоса).
    pub fn set_ducking(&self, on: bool, threshold: f32, level: f32) {
        self.inner.effects.set_ducking(on, threshold, level);
    }

    /// Текущий множитель приглушения (1 — не приглушено).
    pub fn duck_gain(&self) -> f32 {
        self.inner.effects.duck_gain()
    }

    /// Сколько звуков сейчас звучит «в микрофон».
    pub fn virtual_voices(&self) -> u32 {
        self.inner.virtual_shared.voices.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        for b in [&self.inner.monitor_shared, &self.inner.virtual_shared] {
            b.paused.store(paused, Ordering::Relaxed);
        }
    }

    pub fn is_paused(&self) -> bool {
        self.inner.monitor_shared.paused.load(Ordering::Relaxed)
    }

    // ---------- Воспроизведение ----------

    /// Играет звук. `key` — постоянный идентификатор звука (для «начать заново» и «остановить»).
    pub fn play(
        &self,
        key: u64,
        sound: Arc<SoundData>,
        gain: f32,
        target: PlayTarget,
        mode: PlayMode,
    ) -> Result<(), String> {
        self.play_ex(key, sound, gain, target, mode, false)
    }

    /// Как [`Engine::play`], но можно играть по кругу (`looping`) — до команды «стоп».
    pub fn play_ex(
        &self,
        key: u64,
        sound: Arc<SoundData>,
        gain: f32,
        target: PlayTarget,
        mode: PlayMode,
        looping: bool,
    ) -> Result<(), String> {
        self.poll();
        if mode == PlayMode::Toggle && self.is_playing(key) {
            self.stop_key(key);
            return Ok(());
        }
        let restart = mode == PlayMode::Restart;
        let instance = self.inner.next_instance.fetch_add(1, Ordering::Relaxed);
        let buses = self.inner.buses.read();
        let mut sent = 0u32;
        let mut send_to = |bus: &Option<BusHandle>| {
            if let Some(b) = bus {
                let voice = Voice { instance, key, sound: sound.clone(), pos: 0.0, gain, paused: false, looping };
                if b.cmd.try_send(BusCmd::Play { voice, restart }).is_ok() {
                    sent += 1;
                }
            }
        };
        if target.monitor() {
            send_to(&buses.monitor);
        }
        if target.virtual_out() {
            send_to(&buses.virtual_out);
        }
        drop(buses);
        if sent == 0 {
            return Err(match target {
                PlayTarget::Virtual => tr("Виртуальный выход не настроен (Настройки → Устройства)").into(),
                _ => tr("Нет работающего выхода звука").into(),
            });
        }
        self.inner.playing.lock().instances.insert(instance, (key, sent));
        Ok(())
    }

    pub fn stop_key(&self, key: u64) {
        let buses = self.inner.buses.read();
        for b in [&buses.monitor, &buses.virtual_out].into_iter().flatten() {
            let _ = b.cmd.try_send(BusCmd::StopKey(key));
        }
    }

    pub fn stop_all(&self) {
        self.set_paused(false);
        let buses = self.inner.buses.read();
        for b in [&buses.monitor, &buses.virtual_out].into_iter().flatten() {
            let _ = b.cmd.try_send(BusCmd::StopAll);
        }
    }

    /// Разбирает события микшеров (что доиграло).
    pub fn poll(&self) {
        let buses = self.inner.buses.read();
        let mut playing = self.inner.playing.lock();
        for b in [&buses.monitor, &buses.virtual_out].into_iter().flatten() {
            while let Ok(ev) = b.events.try_recv() {
                match ev {
                    BusEvent::Finished { instance, .. } => {
                        if let Some(entry) = playing.instances.get_mut(&instance) {
                            entry.1 = entry.1.saturating_sub(1);
                            if entry.1 == 0 {
                                playing.instances.remove(&instance);
                                playing.positions.retain(|_, (inst, _, _)| *inst != instance);
                            }
                        }
                    }
                    BusEvent::Position { instance, key, secs, paused } => {
                        if playing.instances.contains_key(&instance) {
                            playing.positions.insert(key, (instance, secs, paused));
                        }
                    }
                }
            }
        }
    }

    /// Пауза/продолжение одного звука (на всех выходах).
    pub fn pause_key(&self, key: u64, paused: bool) {
        let buses = self.inner.buses.read();
        for b in [&buses.monitor, &buses.virtual_out].into_iter().flatten() {
            let _ = b.cmd.try_send(BusCmd::PauseKey { key, paused });
        }
        if let Some(p) = self.inner.playing.lock().positions.get_mut(&key) {
            p.2 = paused;
        }
    }

    /// Перемотка звука (секунды от начала файла).
    pub fn seek_key(&self, key: u64, secs: f64) {
        let buses = self.inner.buses.read();
        for b in [&buses.monitor, &buses.virtual_out].into_iter().flatten() {
            let _ = b.cmd.try_send(BusCmd::SeekKey { key, secs });
        }
        if let Some(p) = self.inner.playing.lock().positions.get_mut(&key) {
            p.1 = secs;
        }
    }

    /// Где сейчас играет звук: (секунды, на паузе). `None` — не играет.
    pub fn position(&self, key: u64) -> Option<(f64, bool)> {
        let p = self.inner.playing.lock();
        let any = p.instances.values().any(|(k, _)| *k == key);
        if !any {
            return None;
        }
        Some(p.positions.get(&key).map(|(_, s, paused)| (*s, *paused)).unwrap_or((0.0, false)))
    }

    pub fn is_playing(&self, key: u64) -> bool {
        self.inner.playing.lock().instances.values().any(|(k, _)| *k == key)
    }

    /// Ключи звуков, которые сейчас звучат.
    pub fn playing_keys(&self) -> HashSet<u64> {
        self.poll();
        self.inner.playing.lock().instances.values().map(|(k, _)| *k).collect()
    }

    // ---------- Запись ----------

    /// Начинает запись. `source = None` — устройство по умолчанию (вход или выход для loopback).
    pub fn start_recording(&self, source: Option<DeviceRef>, loopback: bool, path: PathBuf) {
        self.send(Ctl::StartRecording { source, loopback, path });
    }

    pub fn stop_recording(&self) {
        self.send(Ctl::StopRecording);
    }

    pub fn is_recording(&self) -> bool {
        self.inner.status.lock().recording.is_some()
    }

    /// Сколько секунд уже записано.
    pub fn recorded_seconds(&self) -> f64 {
        let frames = self.inner.rec_shared.frames_written.load(Ordering::Relaxed) as f64;
        let rate = self.inner.rec_shared.rate.load(Ordering::Relaxed).max(1) as f64;
        frames / rate
    }

    /// Результаты завершённых записей (забираются один раз).
    pub fn take_recording_results(&self) -> Vec<RecordingResult> {
        std::mem::take(&mut *self.inner.rec_results.lock())
    }

    // ---------- Буфер «что сейчас было» ----------

    /// Начинает держать в памяти последние `secs` секунд источника.
    pub fn start_replay(&self, source: Option<DeviceRef>, loopback: bool, secs: u32) {
        self.send(Ctl::StartReplay { source, loopback, secs });
    }

    pub fn stop_replay(&self) {
        self.send(Ctl::StopReplay);
    }

    pub fn replay_info(&self) -> Option<ReplayInfo> {
        self.inner.replay.lock().as_ref().map(|(sh, dev)| ReplayInfo {
            device: dev.clone(),
            buffered_secs: sh.buffered_secs(),
            capacity_secs: sh.capacity_secs,
            rate: sh.rate,
        })
    }

    /// Последняя ошибка буфера (забирается один раз).
    pub fn take_replay_error(&self) -> Option<String> {
        self.inner.replay_error.lock().take()
    }

    /// Сохраняет последние `secs` секунд в WAV в фоне. Результат придёт в
    /// [`Engine::take_recording_results`].
    pub fn save_replay(&self, secs: f64, path: PathBuf) {
        let Some((shared, _)) = self.inner.replay.lock().clone() else {
            self.inner.rec_results.lock().push(RecordingResult::Failed(tr("буфер выключен").into()));
            return;
        };
        let inner = self.inner.clone();
        std::thread::spawn(move || {
            let res = match crate::replay::save(&shared, secs, &path) {
                Ok(()) => RecordingResult::Saved(path),
                Err(e) => RecordingResult::Failed(tf!("повтор не сохранён: {}", e)),
            };
            log::info!("Сохранение повтора: {res:?}");
            inner.rec_results.lock().push(res);
        });
    }

    pub fn shutdown(&self) {
        self.send(Ctl::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cable_loop_detected() {
        use super::is_cable_loop;
        assert!(is_cable_loop("CABLE Output (VB-Audio Virtual Cable)", "CABLE Input (VB-Audio Virtual Cable)"));
        assert!(!is_cable_loop("Микрофон (Realtek Audio)", "CABLE Input (VB-Audio Virtual Cable)"));
        assert!(!is_cable_loop("CABLE-A Output (VB-Audio Cable A)", "CABLE Input (VB-Audio Virtual Cable)"));
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Levels {
    pub monitor: f32,
    pub virtual_out: f32,
    pub mic: f32,
    pub recording: f32,
}

// ===================== Управляющий поток =====================

struct Streams {
    monitor: Option<cpal::Stream>,
    virtual_out: Option<cpal::Stream>,
    mic: Option<cpal::Stream>,
}

fn err_fn(tx: &mpsc::Sender<Ctl>, which: Which) -> ErrFn {
    let tx = tx.clone();
    let xruns = std::sync::atomic::AtomicU64::new(0);
    Box::new(move |e: cpal::Error| {
        match e.kind() {
            // Не ошибки, а предупреждения: короткий пропуск данных (в loopback-записи Windows
            // сообщает так о каждой паузе в звуке) и отказ дать звуку повышенный приоритет.
            // Поток продолжает работать, останавливать ничего не нужно.
            cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied => {
                let n = xruns.fetch_add(1, Ordering::Relaxed) + 1;
                // В лог пишем не каждый раз, а 1, 2, 4, 8, 16… — чтобы не засорять его.
                if n.is_power_of_two() {
                    log::warn!("Поток {which:?}: {e} (раз: {n})");
                }
            }
            _ => {
                log::error!("Ошибка потока {which:?}: {e}");
                let _ = tx.send(Ctl::StreamFailed { which, message: e.to_string() });
            }
        }
    })
}

fn control_loop(inner: Arc<Inner>, rx: mpsc::Receiver<Ctl>, tx: mpsc::Sender<Ctl>) {
    let mut streams = Streams { monitor: None, virtual_out: None, mic: None };
    let mut recorder: Option<Recorder> = None;
    let mut replay: Option<Replay> = None;
    let mut config = EngineConfig::default();
    let mut retry_at: Option<Instant> = None;
    // Пауза между попытками переподключения растёт до минуты, чтобы не засорять лог.
    let mut retry_delay = Duration::from_secs(3);

    loop {
        let msg = match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(m) => Some(m),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        match msg {
            Some(Ctl::Configure(c)) => {
                config = c;
                retry_delay = Duration::from_secs(3);
                retry_at = apply_config(&inner, &tx, &config, &mut streams).then(|| Instant::now() + retry_delay);
            }
            Some(Ctl::StreamFailed { which, message }) => {
                let mut st = inner.status.lock();
                let slot = match which {
                    Which::Monitor => &mut st.monitor,
                    Which::Virtual => &mut st.virtual_out,
                    Which::Mic => &mut st.mic,
                    Which::Replay => {
                        drop(st);
                        if replay.take().is_some() {
                            *inner.replay.lock() = None;
                            *inner.replay_error.lock() = Some(tf!("буфер остановлен: {}", message));
                        }
                        continue;
                    }
                    Which::Recorder => {
                        drop(st);
                        if let Some(r) = recorder.take() {
                            stop_recorder(&inner, r, Some(message));
                        }
                        continue;
                    }
                };
                slot.active = false;
                slot.error = Some(message);
                // Через пару секунд пробуем переподключиться (например, наушники вставили обратно).
                retry_at.get_or_insert(Instant::now() + retry_delay);
            }
            Some(Ctl::StartRecording { source, loopback, path }) => {
                if let Some(r) = recorder.take() {
                    stop_recorder(&inner, r, None);
                }
                let dir = if loopback { Direction::Output } else { Direction::Input };
                let result = match find_device(source.as_ref(), dir) {
                    Some(dev) => {
                        Recorder::start(&dev, loopback, &path, inner.rec_shared.clone(), err_fn(&tx, Which::Recorder))
                    }
                    None => Err(tr("устройство для записи не найдено").into()),
                };
                match result {
                    Ok(r) => {
                        log::info!("Запись начата: {} → {}", r.device_name, r.path.display());
                        inner.status.lock().recording = Some(RecordingStatus {
                            path: r.path.clone(),
                            device: r.device_name.clone(),
                            started: r.started,
                        });
                        recorder = Some(r);
                    }
                    Err(e) => {
                        log::error!("Запись не началась: {e}");
                        let _ = std::fs::remove_file(&path);
                        inner.rec_results.lock().push(RecordingResult::Failed(e));
                    }
                }
            }
            Some(Ctl::StopRecording) => {
                if let Some(r) = recorder.take() {
                    stop_recorder(&inner, r, None);
                }
            }
            Some(Ctl::StartReplay { source, loopback, secs }) => {
                replay = None;
                *inner.replay.lock() = None;
                let dir = if loopback { Direction::Output } else { Direction::Input };
                let result = match find_device(source.as_ref(), dir) {
                    Some(dev) => Replay::start(&dev, loopback, secs, err_fn(&tx, Which::Replay)),
                    None => Err(tr("устройство для буфера не найдено").into()),
                };
                match result {
                    Ok(r) => {
                        log::info!("Буфер повтора включён: {}, {} с", r.device_name, r.shared.capacity_secs);
                        *inner.replay.lock() = Some((r.shared.clone(), r.device_name.clone()));
                        replay = Some(r);
                    }
                    Err(e) => {
                        log::error!("Буфер повтора не включился: {e}");
                        *inner.replay_error.lock() = Some(e);
                    }
                }
            }
            Some(Ctl::StopReplay) => {
                if replay.take().is_some() {
                    log::info!("Буфер повтора выключен");
                }
                *inner.replay.lock() = None;
            }
            Some(Ctl::Shutdown) => break,
            None => {}
        }

        if let Some(t) = retry_at
            && Instant::now() >= t
        {
            log::info!("Переподключаю аудиоустройства");
            retry_delay = (retry_delay * 2).min(Duration::from_secs(60));
            retry_at = apply_config(&inner, &tx, &config, &mut streams).then(|| Instant::now() + retry_delay);
        }
    }

    if let Some(r) = recorder.take() {
        stop_recorder(&inner, r, None);
    }
    drop(replay);
    drop(streams);
    log::info!("Звуковой движок остановлен");
}

fn stop_recorder(inner: &Inner, r: Recorder, error: Option<String>) {
    inner.status.lock().recording = None;
    let path = r.path.clone();
    let res = match (r.finish(), error) {
        (Ok(p), None) => RecordingResult::Saved(p),
        (Ok(_), Some(e)) => {
            RecordingResult::Failed(tf!("запись прервана: {} (сохранённая часть: {})", e, path.display()))
        }
        (Err(e), _) => RecordingResult::Failed(e),
    };
    log::info!("Запись завершена: {res:?}");
    inner.rec_results.lock().push(res);
}

/// Пересоздаёт выходы. Возвращает `true`, если что-то не открылось (стоит повторить позже).
fn apply_config(inner: &Arc<Inner>, tx: &mpsc::Sender<Ctl>, config: &EngineConfig, streams: &mut Streams) -> bool {
    // Сначала закрываем старое: некоторые драйверы не дают открыть устройство повторно.
    streams.monitor = None;
    streams.virtual_out = None;
    streams.mic = None;
    *inner.buses.write() = Buses::default();
    {
        let mut p = inner.playing.lock();
        p.instances.clear();
        p.positions.clear();
    }

    let mut status = EngineStatus { recording: inner.status.lock().recording.clone(), ..Default::default() };
    let mut buses = Buses::default();
    let mut failed = false;

    // --- Выход «себе» ---
    match open_bus(
        config.monitor.as_ref(),
        None,
        inner.monitor_shared.clone(),
        BusKind::Monitor,
        inner.effects.clone(),
        err_fn(tx, Which::Monitor),
    ) {
        Ok((stream, handle, st)) => {
            streams.monitor = Some(stream);
            buses.monitor = Some(handle);
            status.monitor = st;
        }
        Err(e) => {
            log::error!("Выход «себе» не открылся: {e}");
            failed = true;
            status.monitor = StreamStatus { error: Some(e), ..Default::default() };
        }
    }

    // --- Виртуальный выход + микрофон ---
    if let Some(vref) = config.virtual_out.as_ref() {
        // Микрофон открываем до выхода, чтобы знать его частоту.
        let mut mic_reader = None;
        // Защита от петли: если «микрофоном» выбран выход того же кабеля (CABLE Output при выходе
        // CABLE Input — так бывает, когда VB-Cable сам становится микрофоном по умолчанию), звук
        // пошёл бы по кругу и превратился в гул. Такой микрофон не подключаем.
        let mic_name = find_device(config.mic.as_ref(), Direction::Input).map(|d| device_name(&d));
        let looped = mic_name.as_deref().is_some_and(|m| is_cable_loop(m, &vref.name));
        if config.mic_enabled && looped {
            let m = mic_name.unwrap_or_default();
            log::error!("Микрофон «{m}» — это выход виртуального кабеля «{}»: петля, не подключаю", vref.name);
            status.mic = StreamStatus {
                error: Some(tf!(
                    "«{}» — это выход того же кабеля, звук пошёл бы по кругу (гул). Выберите настоящий микрофон в Настройки → Устройства → «Мой микрофон».",
                    m
                )),
                device: m,
                ..Default::default()
            };
        } else if config.mic_enabled {
            match open_mic(config.mic.as_ref(), err_fn(tx, Which::Mic)) {
                Ok((stream, consumer, rate, st)) => {
                    streams.mic = Some(stream);
                    mic_reader = Some((consumer, rate));
                    status.mic = st;
                }
                Err(e) => {
                    log::error!("Микрофон не открылся: {e}");
                    failed = true;
                    status.mic = StreamStatus { error: Some(e), ..Default::default() };
                }
            }
        }
        match open_bus(
            Some(vref),
            mic_reader,
            inner.virtual_shared.clone(),
            BusKind::Virtual,
            inner.effects.clone(),
            err_fn(tx, Which::Virtual),
        ) {
            Ok((stream, handle, st)) => {
                streams.virtual_out = Some(stream);
                buses.virtual_out = Some(handle);
                status.virtual_out = st;
            }
            Err(e) => {
                log::error!("Виртуальный выход не открылся: {e}");
                failed = true;
                streams.mic = None;
                status.mic.active = false;
                status.virtual_out = StreamStatus { error: Some(e), device: vref.name.clone(), ..Default::default() };
            }
        }
    }

    *inner.buses.write() = buses;
    *inner.status.lock() = status;
    failed
}

/// Микрофон — это другой конец того же виртуального кабеля?
/// «CABLE Output (VB-Audio Virtual Cable)» и «CABLE Input (VB-Audio Virtual Cable)»,
/// «VoiceMeeter Output» и «VoiceMeeter Input» и т. п.
fn is_cable_loop(mic: &str, virtual_out: &str) -> bool {
    let norm = |s: &str| s.to_lowercase().replace("output", "#").replace("input", "#");
    !mic.is_empty() && mic.to_lowercase().contains("output") && norm(mic) == norm(virtual_out)
}

type BusOpen = (cpal::Stream, BusHandle, StreamStatus);

fn open_bus(
    dref: Option<&DeviceRef>,
    mic: Option<(rtrb::Consumer<f32>, u32)>,
    shared: Arc<BusShared>,
    kind: BusKind,
    effects: Arc<EffectsParams>,
    err: ErrFn,
) -> Result<BusOpen, String> {
    let device = find_device(dref, Direction::Output).ok_or_else(|| match dref {
        Some(r) => tf!("устройство «{}» не найдено", r.name),
        None => tr("нет устройства вывода по умолчанию").to_string(),
    })?;
    let name = device_name(&device);
    let config = device.default_output_config().map_err(|e| format!("{name}: {e}"))?;
    let rate = config.sample_rate();
    let (cmd_tx, cmd_rx) = crossbeam_channel::bounded(256);
    let (ev_tx, ev_rx) = crossbeam_channel::bounded(1024);
    let mut renderer = BusRenderer::new(rate, cmd_rx, ev_tx, shared, kind, effects);
    if let Some((consumer, mic_rate)) = mic {
        renderer.mic = Some(MicReader::new(consumer, mic_rate, rate));
    }
    let stream = build_output(&device, config, renderer, err).map_err(|e| format!("{name}: {e}"))?;
    stream.play().map_err(|e| format!("{name}: {e}"))?;
    log::info!("Открыт выход «{name}», {rate} Гц");
    Ok((
        stream,
        BusHandle { cmd: cmd_tx, events: ev_rx },
        StreamStatus { active: true, device: name, rate, error: None },
    ))
}

type MicOpen = (cpal::Stream, rtrb::Consumer<f32>, u32, StreamStatus);

fn open_mic(dref: Option<&DeviceRef>, err: ErrFn) -> Result<MicOpen, String> {
    let device = find_device(dref, Direction::Input).ok_or_else(|| match dref {
        Some(r) => tf!("микрофон «{}» не найден", r.name),
        None => tr("нет микрофона по умолчанию").to_string(),
    })?;
    let name = device_name(&device);
    let config = device.default_input_config().map_err(|e| format!("{name}: {e}"))?;
    let rate = config.sample_rate();
    // Полсекунды буфера — читатель держит задержку около 30 мс.
    let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(rate as usize);
    let mut stereo: Vec<f32> = Vec::with_capacity(8192);
    let stream = build_input(
        &device,
        config,
        move |data, channels| {
            stereo.clear();
            crate::sound::append_as_stereo(data, channels, &mut stereo);
            // Пишем блок целиком или не пишем вовсе, чтобы не перепутать левый и правый каналы.
            if producer.slots() >= stereo.len() {
                for &s in &stereo {
                    let _ = producer.push(s);
                }
            }
        },
        err,
    )
    .map_err(|e| format!("{name}: {e}"))?;
    stream.play().map_err(|e| format!("{name}: {e}"))?;
    log::info!("Открыт микрофон «{name}», {rate} Гц");
    Ok((stream, consumer, rate, StreamStatus { active: true, device: name, rate, error: None }))
}
