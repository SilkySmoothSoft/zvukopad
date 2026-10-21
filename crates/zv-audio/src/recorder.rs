//! Запись с микрофона или «что звучит в устройстве» (loopback) в WAV.
//!
//! Звуковой поток только складывает сэмплы в кольцевой буфер. Запись на диск идёт
//! в отдельном потоке, чтобы медленный диск не вызывал щелчков.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use zv_core::i18n::tr;
use zv_core::tf;

use cpal::traits::{DeviceTrait, StreamTrait};

use crate::mixer::AtomicF32;
use crate::sound::append_as_stereo;
use crate::streams::{ErrFn, build_input};

/// Общие с интерфейсом данные записи.
#[derive(Debug, Default)]
pub struct RecShared {
    pub peak: AtomicF32,
    pub frames_written: AtomicU64,
    pub rate: AtomicU64,
}

pub(crate) struct Recorder {
    _stream: cpal::Stream,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), String>>>,
    pub path: PathBuf,
    pub device_name: String,
    pub started: Instant,
}

impl Recorder {
    /// `loopback = true`: `device` — устройство вывода, записывается то, что в нём звучит.
    pub fn start(
        device: &cpal::Device,
        loopback: bool,
        path: &Path,
        shared: Arc<RecShared>,
        err: ErrFn,
    ) -> Result<Self, String> {
        let config = if loopback { device.default_output_config() } else { device.default_input_config() }
            .map_err(|e| tf!("устройство не отдаёт формат: {}", e))?;
        let rate = config.sample_rate();
        let src_channels = config.channels() as usize;
        let out_channels: u16 = if src_channels == 1 { 1 } else { 2 };

        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| tf!("не удалось создать папку: {}", e))?;
        }
        let spec = hound::WavSpec {
            channels: out_channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer = hound::WavWriter::create(path, spec).map_err(|e| tf!("не удалось создать файл: {}", e))?;

        // Буфер на 4 секунды — с запасом на подвисания диска.
        let capacity = rate as usize * out_channels as usize * 4;
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(capacity);
        shared.frames_written.store(0, Ordering::Relaxed);
        shared.rate.store(rate as u64, Ordering::Relaxed);

        let peak_shared = shared.clone();
        let mut stereo: Vec<f32> = Vec::with_capacity(8192);
        let on_data = move |data: &[f32], channels: usize| {
            let samples: &[f32] = if out_channels == 1 {
                data
            } else {
                stereo.clear();
                append_as_stereo(data, channels, &mut stereo);
                &stereo
            };
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            // Блок пишется целиком или не пишется, чтобы каналы не перепутались.
            if producer.slots() >= samples.len() {
                for &s in samples {
                    let _ = producer.push(s);
                }
            }
            peak_shared.peak.max(peak);
        };

        let stream = build_input(device, config, on_data, err)?;
        stream.play().map_err(|e| tf!("не удалось запустить запись: {}", e))?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let started = Instant::now();
        let thread = std::thread::Builder::new()
            .name("zv-recorder".into())
            .spawn(move || writer_loop(writer, consumer, stop2, shared, rate, out_channels as usize, loopback, started))
            .map_err(|e| e.to_string())?;

        Ok(Self {
            _stream: stream,
            stop,
            thread: Some(thread),
            path: path.to_path_buf(),
            device_name: crate::devices::device_name(device),
            started,
        })
    }

    /// Останавливает запись и дописывает файл.
    pub fn finish(mut self) -> Result<PathBuf, String> {
        self.stop.store(true, Ordering::Relaxed);
        let res = match self.thread.take() {
            Some(t) => t.join().unwrap_or_else(|_| Err(tr("поток записи упал").into())),
            None => Ok(()),
        };
        res.map(|_| self.path.clone())
    }
}

#[allow(clippy::too_many_arguments)]
fn writer_loop(
    mut writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    mut consumer: rtrb::Consumer<f32>,
    stop: Arc<AtomicBool>,
    shared: Arc<RecShared>,
    rate: u32,
    channels: usize,
    loopback: bool,
    started: Instant,
) -> Result<(), String> {
    let mut written_frames: u64 = 0;
    let write = |s: f32, writer: &mut hound::WavWriter<_>| -> Result<(), String> {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer.write_sample(v).map_err(|e| e.to_string())
    };
    loop {
        let stopping = stop.load(Ordering::Relaxed);
        let available = consumer.slots();

        // Loopback в Windows не присылает данные, пока ничего не играет.
        // Чтобы запись не «сжималась», паузы заполняются тишиной по реальному времени.
        if loopback && !stopping {
            let expected = (started.elapsed().as_secs_f64() * rate as f64) as u64;
            let have = written_frames + (available / channels) as u64;
            let slack = rate as u64 / 5; // 200 мс запаса на задержку устройства
            if expected > have + slack {
                let gap = expected - have - slack / 2;
                for _ in 0..gap * channels as u64 {
                    write(0.0, &mut writer)?;
                }
                written_frames += gap;
            }
        }

        let n = available - available % channels;
        if n > 0
            && let Ok(chunk) = consumer.read_chunk(n)
        {
            let (a, b) = chunk.as_slices();
            for &s in a.iter().chain(b.iter()) {
                write(s, &mut writer)?;
            }
            chunk.commit_all();
            written_frames += (n / channels) as u64;
        }
        shared.frames_written.store(written_frames, Ordering::Relaxed);

        if stopping {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    writer.finalize().map_err(|e| e.to_string())
}
