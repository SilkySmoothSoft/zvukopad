//! Микшер одного выхода. Работает в потоке реального времени звуковой карты:
//! здесь нельзя ждать блокировок, ходить на диск и выделять память.
//!
//! Команды приходят через неблокирующий канал, завершённые звуки уходят обратно
//! (вместе со ссылкой на данные, чтобы память освобождалась не в звуковом потоке).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crossbeam_channel::{Receiver, Sender};

use crate::effects::{BusKind, EffectsChain, EffectsParams};
use crate::sound::SoundData;

/// f32, который можно читать и писать из разных потоков без блокировок.
#[derive(Debug, Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    pub fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed)
    }
    /// Запоминает максимум (для индикаторов уровня).
    pub fn max(&self, v: f32) {
        let mut cur = self.0.load(Ordering::Relaxed);
        while v > f32::from_bits(cur) {
            match self.0.compare_exchange_weak(cur, v.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(x) => cur = x,
            }
        }
    }
    /// Забирает значение и обнуляет.
    pub fn take(&self) -> f32 {
        f32::from_bits(self.0.swap(0, Ordering::Relaxed))
    }
}

/// Максимум одновременно звучащих голосов на одном выходе.
pub const MAX_VOICES: usize = 64;

pub(crate) struct Voice {
    pub instance: u64,
    pub key: u64,
    pub sound: Arc<SoundData>,
    /// Позиция в кадрах исходного звука (дробная — для пересчёта частоты).
    pub pos: f64,
    pub gain: f32,
    /// Пауза только этого звука.
    pub paused: bool,
    /// Играть по кругу (паттерны редактора).
    pub looping: bool,
}

pub(crate) enum BusCmd {
    Play {
        voice: Voice,
        restart: bool,
    },
    StopKey(u64),
    StopAll,
    /// Пауза/продолжение одного звука.
    PauseKey {
        key: u64,
        paused: bool,
    },
    /// Перемотка звука на позицию (в секундах исходного файла).
    SeekKey {
        key: u64,
        secs: f64,
    },
}

pub(crate) enum BusEvent {
    /// Голос доиграл или остановлен. `sound` возвращается, чтобы освободить память вне звукового потока.
    Finished {
        instance: u64,
        #[allow(dead_code)]
        sound: Arc<SoundData>,
    },
    /// Где сейчас играет звук (для бегунка), примерно 20 раз в секунду.
    Position { instance: u64, key: u64, secs: f64, paused: bool },
}

/// Общие с интерфейсом величины одного выхода.
#[derive(Debug, Default)]
pub struct BusShared {
    pub volume: AtomicF32,
    /// Пауза: голоса стоят на месте, микрофон продолжает идти.
    pub paused: std::sync::atomic::AtomicBool,
    /// Пиковый уровень с последнего чтения (для индикатора).
    pub peak: AtomicF32,
    pub mic_volume: AtomicF32,
    pub mic_peak: AtomicF32,
    /// Сколько звуков сейчас звучит на этом выходе (для автонажатия рации).
    pub voices: AtomicU32,
}

/// Чтение микрофона, подмешиваемого в выход, с подстройкой частоты.
/// Микрофон и выход — разные устройства с разными «часами», поэтому буфер между ними
/// держится около целевого заполнения: при переполнении лишнее пропускается,
/// при опустошении выдаётся тишина, пока буфер не наберётся снова.
pub(crate) struct MicReader {
    pub consumer: rtrb::Consumer<f32>,
    /// Шаг по входным кадрам на один выходной кадр (частота входа / частота выхода).
    pub ratio: f64,
    /// Целевое заполнение буфера, в стерео-кадрах.
    pub target_frames: usize,
    prev: [f32; 2],
    next: [f32; 2],
    frac: f64,
    priming: bool,
}

impl MicReader {
    pub fn new(consumer: rtrb::Consumer<f32>, in_rate: u32, out_rate: u32) -> Self {
        Self {
            consumer,
            ratio: in_rate as f64 / out_rate as f64,
            target_frames: (in_rate as usize * 30) / 1000, // 30 мс
            prev: [0.0; 2],
            next: [0.0; 2],
            frac: 0.0,
            priming: true,
        }
    }

    fn pop_frame(&mut self) -> Option<[f32; 2]> {
        if self.consumer.slots() < 2 {
            return None;
        }
        let l = self.consumer.pop().ok()?;
        let r = self.consumer.pop().ok()?;
        Some([l, r])
    }

    /// Добавляет микрофон в `out` (стерео). Возвращает пиковый уровень микрофона.
    pub fn mix_into(&mut self, out: &mut [f32], volume: f32) -> f32 {
        let available = self.consumer.slots() / 2;
        if self.priming {
            if available < self.target_frames {
                return 0.0;
            }
            self.priming = false;
        }
        // Слишком большое отставание — выбрасываем лишнее, чтобы не копилась задержка.
        if available > self.target_frames * 4 {
            let drop = available - self.target_frames;
            for _ in 0..drop {
                let _ = self.pop_frame();
            }
        }
        let mut peak = 0.0f32;
        for frame in out.chunks_exact_mut(2) {
            while self.frac >= 1.0 {
                match self.pop_frame() {
                    Some(f) => {
                        self.prev = self.next;
                        self.next = f;
                        self.frac -= 1.0;
                    }
                    None => {
                        self.priming = true;
                        return peak;
                    }
                }
            }
            let t = self.frac as f32;
            let l = (self.prev[0] + (self.next[0] - self.prev[0]) * t) * volume;
            let r = (self.prev[1] + (self.next[1] - self.prev[1]) * t) * volume;
            frame[0] += l;
            frame[1] += r;
            peak = peak.max(l.abs()).max(r.abs());
            self.frac += self.ratio;
        }
        peak
    }
}

/// Микшер одного выхода.
pub(crate) struct BusRenderer {
    pub out_rate: u32,
    pub voices: Vec<Voice>,
    pub cmd_rx: Receiver<BusCmd>,
    pub ev_tx: Sender<BusEvent>,
    pub shared: Arc<BusShared>,
    pub mic: Option<MicReader>,
    pub kind: BusKind,
    pub effects: Arc<EffectsParams>,
    chain: EffectsChain,
    /// Буфер для микрофона (чтобы узнать его уровень до смешивания).
    mic_buf: Vec<f32>,
    /// Сколько ещё блоков держать приглушение после того, как вы замолчали.
    duck_hold: f32,
    /// Кадров с последней отправки позиций.
    pos_tick: u32,
}

impl BusRenderer {
    pub fn new(
        out_rate: u32,
        cmd_rx: Receiver<BusCmd>,
        ev_tx: Sender<BusEvent>,
        shared: Arc<BusShared>,
        kind: BusKind,
        effects: Arc<EffectsParams>,
    ) -> Self {
        Self {
            out_rate,
            voices: Vec::with_capacity(MAX_VOICES),
            cmd_rx,
            ev_tx,
            shared,
            mic: None,
            kind,
            effects,
            chain: EffectsChain::new(out_rate),
            mic_buf: vec![0.0; 16_384],
            duck_hold: 0.0,
            pos_tick: 0,
        }
    }

    fn finish(&self, v: Voice) {
        let _ = self.ev_tx.try_send(BusEvent::Finished { instance: v.instance, sound: v.sound });
    }

    fn handle_commands(&mut self) {
        while let Ok(cmd) = self.cmd_rx.try_recv() {
            match cmd {
                BusCmd::Play { voice, restart } => {
                    if restart {
                        self.stop_where(|v| v.key == voice.key);
                    }
                    if self.voices.len() >= MAX_VOICES {
                        // Нет места — вытесняем самый старый голос.
                        let old = self.voices.remove(0);
                        self.finish(old);
                    }
                    self.voices.push(voice);
                }
                BusCmd::StopKey(key) => self.stop_where(|v| v.key == key),
                BusCmd::StopAll => self.stop_where(|_| true),
                BusCmd::PauseKey { key, paused } => {
                    for v in self.voices.iter_mut().filter(|v| v.key == key) {
                        v.paused = paused;
                    }
                    self.pos_tick = u32::MAX / 2; // сразу сообщить новое состояние
                }
                BusCmd::SeekKey { key, secs } => {
                    for v in self.voices.iter_mut().filter(|v| v.key == key) {
                        let frames = (v.sound.samples.len() / 2) as f64;
                        v.pos = (secs.max(0.0) * v.sound.rate as f64).min((frames - 1.0).max(0.0));
                    }
                    self.pos_tick = u32::MAX / 2;
                }
            }
        }
    }

    fn stop_where(&mut self, pred: impl Fn(&Voice) -> bool) {
        let mut i = 0;
        while i < self.voices.len() {
            if pred(&self.voices[i]) {
                let v = self.voices.swap_remove(i);
                self.finish(v);
            } else {
                i += 1;
            }
        }
    }

    /// Заполняет `out` (стерео, чередование L R) с нуля.
    pub fn render(&mut self, out: &mut [f32]) {
        self.handle_commands();
        out.fill(0.0);

        if !self.shared.paused.load(Ordering::Relaxed) {
            let speed = self.effects.speed_for(self.kind);
            let mut i = 0;
            while i < self.voices.len() {
                if self.voices[i].paused {
                    i += 1;
                    continue;
                }
                let done = render_voice(&mut self.voices[i], self.out_rate, speed, out);
                if done {
                    let v = self.voices.swap_remove(i);
                    self.finish(v);
                } else {
                    i += 1;
                }
            }
        }
        // Эффекты — только для звуков, голос с микрофона идёт чистым.
        self.chain.process(&self.effects, self.kind, out);
        let playing = if self.shared.paused.load(Ordering::Relaxed) {
            0
        } else {
            self.voices.iter().filter(|v| !v.paused).count() as u32
        };
        // Позиции для бегунка — не чаще 20 раз в секунду.
        self.pos_tick = self.pos_tick.saturating_add((out.len() / 2) as u32);
        if self.pos_tick >= self.out_rate / 20 {
            self.pos_tick = 0;
            for v in &self.voices {
                let secs = v.pos / v.sound.rate.max(1) as f64;
                let _ = self.ev_tx.try_send(BusEvent::Position {
                    instance: v.instance,
                    key: v.key,
                    secs,
                    paused: v.paused,
                });
            }
        }
        self.shared.voices.store(playing, Ordering::Relaxed);

        // Микрофон — в отдельный буфер, чтобы сначала узнать уровень (для приглушения).
        let n = out.len().min(self.mic_buf.len());
        let mut mic_peak = 0.0f32;
        let has_mic = self.mic.is_some();
        if let Some(mic) = self.mic.as_mut() {
            self.mic_buf[..n].fill(0.0);
            mic_peak = mic.mix_into(&mut self.mic_buf[..n], self.shared.mic_volume.get());
            self.shared.mic_peak.max(mic_peak);
        }
        if has_mic {
            self.update_duck(mic_peak, n / 2);
        }
        let duck = self.effects.duck_gain();
        if duck < 0.999 {
            for s in out.iter_mut() {
                *s *= duck;
            }
        }
        if has_mic {
            for (o, m) in out[..n].iter_mut().zip(&self.mic_buf[..n]) {
                *o += *m;
            }
        }

        let vol = self.shared.volume.get();
        let mut peak = 0.0f32;
        for s in out.iter_mut() {
            *s = (*s * vol).clamp(-1.0, 1.0);
            peak = peak.max(s.abs());
        }
        self.shared.peak.max(peak);
    }
}

impl BusRenderer {
    /// Плавно меняет множитель приглушения: быстро вниз (30 мс), медленно вверх (400 мс).
    fn update_duck(&mut self, mic_peak: f32, frames: usize) {
        use std::sync::atomic::Ordering;
        if !self.effects.duck_on.load(Ordering::Relaxed) {
            return;
        }
        let block = frames as f32 / self.out_rate.max(1) as f32;
        if mic_peak > self.effects.duck_threshold.get() {
            self.duck_hold = 0.3;
        } else {
            self.duck_hold -= block;
        }
        let target = if self.duck_hold > 0.0 { self.effects.duck_level.get() } else { 1.0 };
        let cur = self.effects.duck_gain.get();
        let time = if target < cur { 0.03 } else { 0.4 };
        let k = (block / time).min(1.0);
        self.effects.duck_gain.set(cur + (target - cur) * k);
    }
}

impl Drop for BusRenderer {
    fn drop(&mut self) {
        if self.mic.is_some() {
            self.effects.duck_gain.set(1.0);
        }
        self.shared.voices.store(0, Ordering::Relaxed);
    }
}

/// Добавляет голос в буфер. Возвращает `true`, если звук закончился.
fn render_voice(v: &mut Voice, out_rate: u32, speed: f64, out: &mut [f32]) -> bool {
    let data = &v.sound.samples;
    let frames = data.len() / 2;
    if frames == 0 {
        return true;
    }
    let step = v.sound.rate as f64 / out_rate as f64 * speed;
    for frame in out.chunks_exact_mut(2) {
        let mut idx = v.pos as usize;
        if idx >= frames {
            if !v.looping {
                return true;
            }
            v.pos -= frames as f64;
            idx = (v.pos.max(0.0) as usize).min(frames - 1);
        }
        let t = (v.pos - idx as f64) as f32;
        let (l0, r0) = (data[idx * 2], data[idx * 2 + 1]);
        let (l1, r1) = if idx + 1 < frames { (data[idx * 2 + 2], data[idx * 2 + 3]) } else { (l0, r0) };
        frame[0] += (l0 + (l1 - l0) * t) * v.gain;
        frame[1] += (r0 + (r1 - r0) * t) * v.gain;
        v.pos += step;
    }
    !v.looping && v.pos as usize >= frames
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus(rate: u32) -> (BusRenderer, Sender<BusCmd>, Receiver<BusEvent>) {
        let (ctx, crx) = crossbeam_channel::bounded(16);
        let (etx, erx) = crossbeam_channel::bounded(16);
        let shared =
            Arc::new(BusShared { volume: AtomicF32::new(1.0), mic_volume: AtomicF32::new(1.0), ..Default::default() });
        (BusRenderer::new(rate, crx, etx, shared, BusKind::Monitor, Arc::new(EffectsParams::default())), ctx, erx)
    }

    #[test]
    fn plays_and_finishes_with_resampling() {
        let (mut b, tx, rx) = bus(48_000);
        // 24 кГц, 100 кадров → на 48 кГц ~200 кадров.
        let sound = Arc::new(SoundData::from_stereo(vec![0.5; 200], 24_000));
        tx.send(BusCmd::Play {
            voice: Voice { instance: 1, key: 7, sound, pos: 0.0, gain: 1.0, paused: false, looping: false },
            restart: false,
        })
        .unwrap();
        let mut out = vec![0.0; 256];
        b.render(&mut out);
        assert!((out[0] - 0.5).abs() < 1e-6);
        assert!(next_finished(&rx).is_none());
        b.render(&mut out);
        assert_eq!(next_finished(&rx), Some(1));
        assert!(b.voices.is_empty());
    }

    #[test]
    fn restart_replaces_same_key() {
        let (mut b, tx, rx) = bus(48_000);
        let sound = Arc::new(SoundData::from_stereo(vec![0.1; 48_000], 48_000));
        for inst in 1..=2 {
            tx.send(BusCmd::Play {
                voice: Voice {
                    instance: inst,
                    key: 1,
                    sound: sound.clone(),
                    pos: 0.0,
                    gain: 1.0,
                    paused: false,
                    looping: false,
                },
                restart: true,
            })
            .unwrap();
        }
        let mut out = vec![0.0; 64];
        b.render(&mut out);
        assert_eq!(b.voices.len(), 1);
        assert_eq!(b.voices[0].instance, 2);
        assert_eq!(next_finished(&rx), Some(1));
    }

    /// Первое событие «доиграл» (сообщения о позиции пропускаются).
    fn next_finished(rx: &Receiver<BusEvent>) -> Option<u64> {
        rx.try_iter().find_map(|e| match e {
            BusEvent::Finished { instance, .. } => Some(instance),
            _ => None,
        })
    }

    #[test]
    fn pause_and_seek_one_voice() {
        let (mut b, tx, _rx) = bus(48_000);
        let sound = Arc::new(SoundData::from_stereo(vec![0.1; 96_000], 48_000));
        tx.send(BusCmd::Play {
            voice: Voice { instance: 1, key: 5, sound, pos: 0.0, gain: 1.0, paused: false, looping: false },
            restart: false,
        })
        .unwrap();
        let mut out = vec![0.0; 64];
        b.render(&mut out);
        let p = b.voices[0].pos;
        tx.send(BusCmd::PauseKey { key: 5, paused: true }).unwrap();
        b.render(&mut out);
        assert_eq!(b.voices[0].pos, p);
        assert_eq!(out[0], 0.0);
        tx.send(BusCmd::SeekKey { key: 5, secs: 0.5 }).unwrap();
        b.render(&mut out);
        assert_eq!(b.voices[0].pos, 24_000.0);
    }

    #[test]
    fn mic_reader_passes_audio() {
        let (mut prod, cons) = rtrb::RingBuffer::<f32>::new(48_000);
        for _ in 0..4_800 {
            prod.push(0.25).unwrap();
        }
        let mut mic = MicReader::new(cons, 48_000, 48_000);
        let mut out = vec![0.0; 128];
        let peak = mic.mix_into(&mut out, 1.0);
        assert!(peak > 0.2);
        assert!((out[100] - 0.25).abs() < 1e-6);
    }
}
