//! Сведение паттерна редактора в один звук (для прослушивания по кругу и сохранения в WAV).

use std::collections::HashMap;
use std::sync::Arc;

use zv_audio::SoundData;
use zv_core::pattern::Pattern;

/// Частота готового звука.
pub const RATE: u32 = 48_000;

/// Длина звука клипа в долях (с учётом высоты).
pub fn clip_beats(p: &Pattern, data: &SoundData, speed: f64) -> f64 {
    data.duration_secs() / speed.max(0.01) / p.beat_secs()
}

/// Сводит паттерн. `sounds` — загруженные звуки по id. Возвращает звук и список id, которых не хватило.
///
/// По кругу (`looping`) длина ровно равна паттерну, а «хвосты» звуков из конца переходят в начало —
/// так петля звучит без шва. Без круга звук длится, пока не доиграет последний клип.
pub fn render(p: &Pattern, sounds: &HashMap<String, Arc<SoundData>>) -> (SoundData, Vec<String>) {
    let beat = p.beat_secs();
    let loop_frames = ((p.length_secs() * RATE as f64).round() as usize).max(1);
    let mut missing = Vec::new();
    // Длина без круга — до конца последнего клипа.
    let mut total = loop_frames;
    if !p.looping {
        for t in p.tracks.iter().filter(|t| !t.muted) {
            for c in &t.clips {
                if let Some(d) = sounds.get(&c.sound) {
                    let end = c.start * beat + d.duration_secs() / c.speed();
                    total = total.max((end * RATE as f64).ceil() as usize);
                }
            }
        }
    }
    let mut out = vec![0.0f32; total * 2];
    for t in p.tracks.iter().filter(|t| !t.muted) {
        for c in &t.clips {
            let Some(d) = sounds.get(&c.sound) else {
                if !c.sound.is_empty() && !missing.contains(&c.sound) {
                    missing.push(c.sound.clone());
                }
                continue;
            };
            let src = &d.samples;
            let src_frames = src.len() / 2;
            if src_frames == 0 {
                continue;
            }
            let gain = c.volume * t.volume * d.norm_gain;
            let step = d.rate as f64 / RATE as f64 * c.speed();
            let start = (c.start * beat * RATE as f64).round() as usize;
            let mut pos = 0.0f64;
            let mut n = 0usize;
            loop {
                let idx = pos as usize;
                if idx >= src_frames {
                    break;
                }
                let mut o = start + n;
                if p.looping {
                    o %= total;
                } else if o >= total {
                    break;
                }
                let fr = (pos - idx as f64) as f32;
                let next = (idx + 1).min(src_frames - 1);
                let l = src[idx * 2] + (src[next * 2] - src[idx * 2]) * fr;
                let r = src[idx * 2 + 1] + (src[next * 2 + 1] - src[idx * 2 + 1]) * fr;
                out[o * 2] += l * gain;
                out[o * 2 + 1] += r * gain;
                pos += step;
                n += 1;
                // Звук длиннее целой петли — дальше накладывать на себя не нужно.
                if p.looping && n >= total {
                    break;
                }
            }
        }
    }
    // Без перегруза: если сумма вышла за предел — делаем тише целиком.
    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if peak > 0.98 {
        let k = 0.98 / peak;
        out.iter_mut().for_each(|v| *v *= k);
    }
    let mut data = SoundData::from_stereo(out, RATE);
    // Громкость уже учтена по звукам — повторно не выравниваем.
    data.norm_gain = 1.0;
    (data, missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zv_core::pattern::{Clip, Track};

    #[test]
    fn places_clips_on_beats_and_wraps() {
        let mut p = Pattern { bpm: 60.0, bars: 1, beats_per_bar: 4, ..Default::default() };
        p.tracks = vec![Track {
            clips: vec![
                Clip { sound: "a".into(), start: 1.0, ..Default::default() },
                Clip { sound: "a".into(), start: 3.5, ..Default::default() },
                Clip { sound: "нет".into(), start: 0.0, ..Default::default() },
            ],
            ..Default::default()
        }];
        let mut a = SoundData::from_stereo(vec![0.5; RATE as usize * 2], RATE); // 1 секунда
        a.norm_gain = 1.0;
        let sounds = HashMap::from([("a".to_string(), Arc::new(a))]);
        let (d, missing) = render(&p, &sounds);
        assert_eq!(missing, vec!["нет".to_string()]);
        assert_eq!(d.frames(), 4 * RATE as usize);
        let at = |sec: f64| d.samples[(sec * RATE as f64) as usize * 2];
        assert_eq!(at(0.5), 0.0);
        assert!(at(1.5) > 0.4);
        assert!(at(0.2) > 0.4, "хвост звука с 3.5 перешёл в начало петли");
    }
}
