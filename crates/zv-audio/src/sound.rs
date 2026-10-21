//! Декодирование звуковых файлов в память и оценка громкости.

use std::path::Path;
use zv_core::i18n::tr;
use zv_core::tf;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// Самый длинный звук, который загружается целиком (защита от случайно брошенного фильма).
pub const MAX_SECONDS: f64 = 15.0 * 60.0;

/// Звук, полностью декодированный в память: стерео, f32, чередование L R L R…
#[derive(Debug)]
pub struct SoundData {
    pub samples: Vec<f32>,
    pub rate: u32,
    /// Множитель, приводящий звук к общей громкости (используется, если включено выравнивание).
    pub norm_gain: f32,
    pub peak: f32,
}

impl SoundData {
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / self.rate.max(1) as f64
    }

    /// Размер в памяти, байт.
    pub fn memory_bytes(&self) -> usize {
        self.samples.len() * std::mem::size_of::<f32>()
    }

    /// Кусок звука с `start` по `end` (в кадрах) с плавным появлением и затуханием
    /// длиной `fade` кадров — чтобы на краях не было щелчков.
    pub fn slice(&self, start: usize, end: usize, fade: usize) -> SoundData {
        let frames = self.frames();
        let end = end.min(frames);
        let start = start.min(end);
        let mut out = self.samples[start * 2..end * 2].to_vec();
        let n = end - start;
        let fade = fade.min(n / 2);
        for i in 0..fade {
            let k = i as f32 / fade as f32;
            out[i * 2] *= k;
            out[i * 2 + 1] *= k;
            let j = n - 1 - i;
            out[j * 2] *= k;
            out[j * 2 + 1] *= k;
        }
        SoundData::from_stereo(out, self.rate)
    }

    /// Сводка для рисования волны: для каждого из `buckets` отрезков — (минимум, максимум).
    pub fn peaks(&self, buckets: usize) -> Vec<(f32, f32)> {
        let frames = self.frames();
        let buckets = buckets.clamp(1, frames.max(1));
        let mut out = Vec::with_capacity(buckets);
        for b in 0..buckets {
            let from = b * frames / buckets;
            let to = ((b + 1) * frames / buckets).max(from + 1).min(frames);
            let (mut lo, mut hi) = (0.0f32, 0.0f32);
            for f in from..to {
                let v = (self.samples[f * 2] + self.samples[f * 2 + 1]) * 0.5;
                lo = lo.min(v);
                hi = hi.max(v);
            }
            out.push((lo, hi));
        }
        out
    }

    /// Записывает звук в WAV (16 бит, стерео). Сначала во временный файл, потом переименовывает.
    pub fn write_wav(&self, path: &Path) -> Result<(), String> {
        let tmp = path.with_extension("tmp-zv");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: self.rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let write = || -> Result<(), hound::Error> {
            let mut w = hound::WavWriter::create(&tmp, spec)?;
            for &s in &self.samples {
                w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
            }
            w.finalize()
        };
        if let Err(e) = write() {
            let _ = std::fs::remove_file(&tmp);
            return Err(tf!("не удалось записать файл: {}", e));
        }
        std::fs::rename(&tmp, path).map_err(|e| tf!("не удалось сохранить файл: {}", e))
    }

    /// Создаёт звук из готовых стерео-сэмплов и сразу считает громкость.
    pub fn from_stereo(samples: Vec<f32>, rate: u32) -> Self {
        let (norm_gain, peak) = loudness_gain(&samples, rate);
        Self { samples, rate, norm_gain, peak }
    }
}

/// Декодирует файл (wav, mp3, ogg, flac, m4a/aac) в стерео f32.
pub fn decode_file(path: &Path) -> Result<SoundData, String> {
    let file = std::fs::File::open(path).map_err(|e| tf!("не удалось открыть файл: {}", e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| tf!("формат не распознан: {}", e))?;

    let track = format.default_track(TrackType::Audio).ok_or(tr("в файле нет звуковой дорожки"))?;
    let track_id = track.id;
    let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or(tr("нет параметров звуковой дорожки"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| tf!("кодек не поддерживается: {}", e))?;

    let mut out: Vec<f32> = Vec::new();
    let mut tmp: Vec<f32> = Vec::new();
    let mut rate: u32 = 0;

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymError::ResetRequired) => break,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                if out.is_empty() {
                    return Err(tf!("ошибка чтения: {}", e));
                }
                break;
            }
        };
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(b) => b,
            Err(SymError::DecodeError(_)) | Err(SymError::IoError(_)) => continue,
            Err(e) => return Err(tf!("ошибка декодирования: {}", e)),
        };
        let spec = buf.spec();
        if rate == 0 {
            rate = spec.rate();
        }
        let channels = spec.channels().count().max(1);
        tmp.resize(buf.samples_interleaved(), 0.0);
        buf.copy_to_slice_interleaved(&mut tmp);
        append_as_stereo(&tmp, channels, &mut out);

        if rate > 0 && (out.len() / 2) as f64 / rate as f64 > MAX_SECONDS {
            log::warn!("{}: звук длиннее {} мин, обрезан", path.display(), MAX_SECONDS / 60.0);
            break;
        }
    }

    if rate == 0 || out.is_empty() {
        return Err(tr("файл пустой или не содержит звука").into());
    }
    Ok(SoundData::from_stereo(out, rate))
}

/// Добавляет кадры с `channels` каналами в стерео-буфер. Моно дублируется, многоканальный
/// звук сводится: левые каналы — влево, правые — вправо (упрощённо: первые два + центр).
pub fn append_as_stereo(src: &[f32], channels: usize, out: &mut Vec<f32>) {
    match channels {
        1 => {
            out.reserve(src.len() * 2);
            for &s in src {
                out.push(s);
                out.push(s);
            }
        }
        2 => out.extend_from_slice(src),
        n => {
            out.reserve(src.len() / n * 2);
            for frame in src.chunks_exact(n) {
                // L, R, C, … — центр делим поровну.
                let c = if n >= 3 { frame[2] * 0.707 } else { 0.0 };
                out.push(frame[0] + c);
                out.push(frame[1] + c);
            }
        }
    }
}

/// Оценка громкости: RMS по блокам 50 мс, тихие блоки (тише −50 дБ) не учитываются.
/// Возвращает (множитель до целевой громкости, пиковое значение).
pub fn loudness_gain(samples: &[f32], rate: u32) -> (f32, f32) {
    const TARGET_RMS: f32 = 0.12; // примерно −18 dBFS
    const GATE: f32 = 0.00316; // −50 dBFS
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak < 1e-6 {
        return (1.0, 0.0);
    }
    let block = ((rate as usize / 20).max(1)) * 2;
    let mut energy = 0.0f64;
    let mut count = 0usize;
    for chunk in samples.chunks(block) {
        let e: f64 = chunk.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / chunk.len() as f64;
        if e.sqrt() as f32 > GATE {
            energy += e;
            count += 1;
        }
    }
    if count == 0 {
        return (1.0, peak);
    }
    let rms = (energy / count as f64).sqrt() as f32;
    let mut gain = (TARGET_RMS / rms).clamp(0.1, 4.0);
    // Не даём пику уйти выше 0.98 после усиления.
    if gain * peak > 0.98 {
        gain = 0.98 / peak;
    }
    (gain, peak)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_fade_and_wav() {
        let s = SoundData::from_stereo(vec![0.5; 2000], 8000);
        let part = s.slice(100, 600, 10);
        assert_eq!(part.frames(), 500);
        assert_eq!(part.samples[0], 0.0);
        assert!((part.samples[500] - 0.5).abs() < 1e-6);
        assert_eq!(s.peaks(10).len(), 10);
        let path = std::env::temp_dir().join(format!("zv-slice-{}.wav", std::process::id()));
        part.write_wav(&path).unwrap();
        let back = decode_file(&path).unwrap();
        assert_eq!(back.frames(), 500);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn mono_to_stereo() {
        let mut out = Vec::new();
        append_as_stereo(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, vec![0.1, 0.1, 0.2, 0.2]);
    }

    #[test]
    fn quiet_sound_gets_louder_but_not_clipped() {
        let rate = 48_000;
        let samples: Vec<f32> = (0..rate * 2).map(|i| ((i as f32) * 0.05).sin() * 0.02).collect();
        let (gain, peak) = loudness_gain(&samples, rate as u32);
        assert!(gain > 1.0);
        assert!(gain * peak <= 0.981);
    }

    #[test]
    fn decode_wav_roundtrip() {
        let path = std::env::temp_dir().join(format!("zv-decode-{}.wav", std::process::id()));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 22_050,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..22_050 {
            w.write_sample(((i as f32 * 0.1).sin() * 10_000.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        let s = decode_file(&path).unwrap();
        assert_eq!(s.rate, 22_050);
        assert_eq!(s.frames(), 22_050);
        assert!((s.duration_secs() - 1.0).abs() < 0.01);
        std::fs::remove_file(&path).unwrap();
    }
}
