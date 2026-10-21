//! Буфер «что сейчас было»: последние N секунд выбранного источника держатся в памяти
//! (на диск ничего не пишется), по команде сохраняются в WAV.
//!
//! Работает, только пока включён. 60 секунд стерео 48 кГц ≈ 23 МБ памяти.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use zv_core::i18n::tr;
use zv_core::tf;

use cpal::traits::{DeviceTrait, StreamTrait};
use parking_lot::Mutex;

use crate::sound::{SoundData, append_as_stereo};
use crate::streams::{ErrFn, build_input};

/// Кольцевой буфер стерео-сэмплов.
struct Ring {
    buf: Vec<f32>,
    /// Куда писать следующий сэмпл.
    pos: usize,
    /// Сколько сэмплов уже заполнено (не больше длины буфера).
    filled: usize,
}

impl Ring {
    fn new(samples: usize) -> Self {
        Self { buf: vec![0.0; samples.max(2)], pos: 0, filled: 0 }
    }

    fn push_slice(&mut self, data: &[f32]) {
        for &s in data {
            self.buf[self.pos] = s;
            self.pos = (self.pos + 1) % self.buf.len();
        }
        self.filled = (self.filled + data.len()).min(self.buf.len());
    }

    fn push_silence(&mut self, n: usize) {
        for _ in 0..n {
            self.buf[self.pos] = 0.0;
            self.pos = (self.pos + 1) % self.buf.len();
        }
        self.filled = (self.filled + n).min(self.buf.len());
    }

    /// Последние `n` сэмплов по порядку.
    fn last(&self, n: usize) -> Vec<f32> {
        let n = n.min(self.filled) & !1; // целое число кадров
        let len = self.buf.len();
        let start = (self.pos + len - n) % len;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            out.push(self.buf[(start + i) % len]);
        }
        out
    }
}

pub(crate) struct ReplayShared {
    ring: Mutex<Ring>,
    pub rate: u32,
    pub capacity_secs: u32,
    frames_total: AtomicU64,
}

impl ReplayShared {
    /// Сколько секунд уже накоплено.
    pub fn buffered_secs(&self) -> f64 {
        self.ring.lock().filled as f64 / 2.0 / self.rate.max(1) as f64
    }

    /// Последние `secs` секунд (или всё, что есть).
    pub fn snapshot(&self, secs: f64) -> SoundData {
        let n = (secs.clamp(0.0, self.capacity_secs as f64 + 1.0) * self.rate as f64) as usize * 2;
        SoundData::from_stereo(self.ring.lock().last(n), self.rate)
    }
}

pub(crate) struct Replay {
    _stream: cpal::Stream,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub shared: Arc<ReplayShared>,
    pub device_name: String,
}

impl Replay {
    pub fn start(device: &cpal::Device, loopback: bool, secs: u32, err: ErrFn) -> Result<Self, String> {
        let config = if loopback { device.default_output_config() } else { device.default_input_config() }
            .map_err(|e| tf!("устройство не отдаёт формат: {}", e))?;
        let rate = config.sample_rate();
        let secs = secs.clamp(5, 600);
        let shared = Arc::new(ReplayShared {
            ring: Mutex::new(Ring::new(rate as usize * 2 * secs as usize)),
            rate,
            capacity_secs: secs,
            frames_total: AtomicU64::new(0),
        });
        let (mut producer, mut consumer) = rtrb::RingBuffer::<f32>::new(rate as usize * 2 * 2);
        let mut stereo: Vec<f32> = Vec::with_capacity(8192);
        let stream = build_input(
            device,
            config,
            move |data, channels| {
                stereo.clear();
                append_as_stereo(data, channels, &mut stereo);
                if producer.slots() >= stereo.len() {
                    for &s in &stereo {
                        let _ = producer.push(s);
                    }
                }
            },
            err,
        )?;
        stream.play().map_err(|e| tf!("не удалось запустить буфер: {}", e))?;

        let stop = Arc::new(AtomicBool::new(false));
        let (stop2, shared2) = (stop.clone(), shared.clone());
        let thread = std::thread::Builder::new()
            .name("zv-replay".into())
            .spawn(move || {
                let started = Instant::now();
                let mut tmp: Vec<f32> = Vec::with_capacity(rate as usize);
                while !stop2.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(20));
                    let n = consumer.slots() & !1;
                    tmp.clear();
                    if n > 0
                        && let Ok(chunk) = consumer.read_chunk(n)
                    {
                        let (a, b) = chunk.as_slices();
                        tmp.extend_from_slice(a);
                        tmp.extend_from_slice(b);
                        chunk.commit_all();
                    }
                    let mut ring = shared2.ring.lock();
                    // Loopback молчит, пока ничего не играет — заполняем паузы тишиной по времени.
                    if loopback {
                        let expected = (started.elapsed().as_secs_f64() * rate as f64) as u64;
                        let have = shared2.frames_total.load(Ordering::Relaxed) + (tmp.len() / 2) as u64;
                        let slack = rate as u64 / 5;
                        if expected > have + slack {
                            let gap = expected - have - slack / 2;
                            ring.push_silence(gap as usize * 2);
                            shared2.frames_total.fetch_add(gap, Ordering::Relaxed);
                        }
                    }
                    ring.push_slice(&tmp);
                    shared2.frames_total.fetch_add((tmp.len() / 2) as u64, Ordering::Relaxed);
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(Self {
            _stream: stream,
            stop,
            thread: Some(thread),
            shared,
            device_name: crate::devices::device_name(device),
        })
    }
}

impl Drop for Replay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Сохраняет последние `secs` секунд в WAV (в отдельном потоке вызывающего).
pub(crate) fn save(shared: &ReplayShared, secs: f64, path: &Path) -> Result<(), String> {
    let data = shared.snapshot(secs);
    if data.frames() < (shared.rate / 10) as usize {
        return Err(tr("буфер ещё пуст — подождите немного").into());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    data.write_wav(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_keeps_last() {
        let mut r = Ring::new(6);
        r.push_slice(&[1.0, 1.0, 2.0, 2.0]);
        r.push_slice(&[3.0, 3.0, 4.0, 4.0]);
        assert_eq!(r.last(100), vec![2.0, 2.0, 3.0, 3.0, 4.0, 4.0]);
        assert_eq!(r.last(2), vec![4.0, 4.0]);
        r.push_silence(2);
        assert_eq!(r.last(4), vec![4.0, 4.0, 0.0, 0.0]);
    }

    #[test]
    fn save_last_seconds() {
        let rate = 1000;
        let shared = ReplayShared {
            ring: Mutex::new(Ring::new(rate as usize * 2 * 5)),
            rate,
            capacity_secs: 5,
            frames_total: AtomicU64::new(0),
        };
        // 7 секунд в буфер на 5 — остаются последние 5.
        let data: Vec<f32> = (0..rate * 7).flat_map(|i| [i as f32 / 10_000.0; 2]).collect();
        shared.ring.lock().push_slice(&data);
        assert!((shared.buffered_secs() - 5.0).abs() < 1e-9);
        assert_eq!(shared.snapshot(2.0).frames(), 2000);
        assert_eq!(shared.snapshot(f64::MAX).frames(), 5000);
        let path = std::env::temp_dir().join(format!("zv-replay-{}.wav", std::process::id()));
        save(&shared, 3.0, &path).unwrap();
        let back = crate::sound::decode_file(&path).unwrap();
        assert_eq!(back.frames(), 3000);
        let _ = std::fs::remove_file(&path);
    }
}
