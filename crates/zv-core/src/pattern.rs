//! Паттерн простого редактора: дорожки, на них звуки из библиотеки, привязанные к сетке.
//!
//! Время меряется в долях (beat, четверть ноты). Длина паттерна — `bars × beats_per_bar` долей,
//! темп — `bpm` долей в минуту. Паттерн хранится в `%APPDATA%\Zvukopad\patterns\<имя>.json`.

use serde::{Deserialize, Serialize};

/// Звук на дорожке.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clip {
    /// Звук из библиотеки (относительный путь в папке звуков).
    pub sound: String,
    /// Начало, в долях от начала паттерна.
    pub start: f64,
    /// Высота в полутонах (−12…+12). Меняет и скорость, как у пластинки.
    pub pitch: f32,
    /// Громкость, 1.0 = 100 %.
    pub volume: f32,
}

impl Default for Clip {
    fn default() -> Self {
        Self { sound: String::new(), start: 0.0, pitch: 0.0, volume: 1.0 }
    }
}

impl Clip {
    /// Во сколько раз быстрее играет звук из-за смены высоты.
    pub fn speed(&self) -> f64 {
        2f64.powf(self.pitch.clamp(-24.0, 24.0) as f64 / 12.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Track {
    pub name: String,
    pub volume: f32,
    pub muted: bool,
    pub clips: Vec<Clip>,
}

impl Default for Track {
    fn default() -> Self {
        Self { name: String::new(), volume: 1.0, muted: false, clips: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pattern {
    pub name: String,
    pub bpm: f32,
    pub bars: u32,
    pub beats_per_bar: u32,
    /// Шаг сетки в долях: 4 = такт (при 4/4), 1 = четверть, 0.5 = восьмая, 0.25 = шестнадцатая. 0 — без сетки.
    pub grid: f64,
    pub looping: bool,
    pub tracks: Vec<Track>,
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            name: String::new(),
            bpm: 120.0,
            bars: 4,
            beats_per_bar: 4,
            grid: 0.5,
            looping: true,
            tracks: (1..=4).map(|_| Track::default()).collect(),
        }
    }
}

impl Pattern {
    /// Длительность одной доли, секунд.
    pub fn beat_secs(&self) -> f64 {
        60.0 / self.bpm.clamp(20.0, 400.0) as f64
    }

    /// Длина паттерна в долях.
    pub fn length_beats(&self) -> f64 {
        (self.bars.max(1) * self.beats_per_bar.max(1)) as f64
    }

    pub fn length_secs(&self) -> f64 {
        self.length_beats() * self.beat_secs()
    }

    /// Привязка к сетке.
    pub fn snap(&self, beat: f64) -> f64 {
        let b = beat.max(0.0);
        if self.grid <= 0.0 { b } else { (b / self.grid).round() * self.grid }
    }

    /// Привязка вниз (для клика: звук ставится в ту клетку, по которой кликнули).
    pub fn snap_floor(&self, beat: f64) -> f64 {
        let b = beat.max(0.0);
        if self.grid <= 0.0 { b } else { (b / self.grid + 1e-9).floor() * self.grid }
    }

    /// Все звуки, которые используются в паттерне.
    pub fn sounds(&self) -> Vec<String> {
        let mut v: Vec<String> = self.tracks.iter().flat_map(|t| t.clips.iter().map(|c| c.sound.clone())).collect();
        v.sort();
        v.dedup();
        v
    }
}

/// Варианты шага сетки для выбора в интерфейсе: (шаг в долях, подпись).
pub const GRID_STEPS: [(f64, &str); 6] =
    [(4.0, "1 такт"), (1.0, "1/4"), (0.5, "1/8"), (0.25, "1/16"), (0.125, "1/32"), (0.0, "без сетки")];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapping_and_length() {
        let p = Pattern::default();
        assert_eq!(p.length_beats(), 16.0);
        assert!((p.length_secs() - 8.0).abs() < 1e-9);
        assert_eq!(p.snap(1.3), 1.5);
        assert_eq!(p.snap_floor(1.3), 1.0);
        assert!((Clip { pitch: 12.0, ..Default::default() }.speed() - 2.0).abs() < 1e-9);
        let back: Pattern = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back, p);
    }
}
