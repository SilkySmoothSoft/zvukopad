//! Эффекты: реверберация, эхо, «рация», перегруз, «8 бит», «робот», скорость.
#![allow(clippy::needless_range_loop)] // в звуковых циклах индекс канала читается проще
//!
//! Параметры живут в [`EffectsParams`] (атомарные числа — их меняет интерфейс), а состояние
//! (буферы задержек и т. п.) — в [`EffectsChain`], своя цепочка у каждого выхода.
//! Все буферы выделяются заранее: в звуковом потоке память не выделяется.

use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};

use zv_core::effects::EffectsSettings;

use crate::mixer::AtomicF32;

/// Какой это выход.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusKind {
    Monitor,
    Virtual,
}

#[derive(Debug, Default)]
pub struct EffectsParams {
    enabled: AtomicBool,
    on_monitor: AtomicBool,
    on_virtual: AtomicBool,
    speed_on: AtomicBool,
    speed: AtomicF32,
    reverb_on: AtomicBool,
    reverb_room: AtomicF32,
    reverb_damp: AtomicF32,
    reverb_mix: AtomicF32,
    echo_on: AtomicBool,
    echo_ms: AtomicF32,
    echo_feedback: AtomicF32,
    echo_mix: AtomicF32,
    radio_on: AtomicBool,
    distortion_on: AtomicBool,
    distortion_drive: AtomicF32,
    bitcrush_on: AtomicBool,
    bitcrush_bits: AtomicF32,
    bitcrush_downsample: AtomicF32,
    robot_on: AtomicBool,
    robot_hz: AtomicF32,
    /// Приглушение звуков, пока вы говорите.
    pub(crate) duck_on: AtomicBool,
    /// Порог уровня микрофона (0..1).
    pub(crate) duck_threshold: AtomicF32,
    /// Громкость звуков, пока вы говорите (0..1).
    pub(crate) duck_level: AtomicF32,
    /// Текущий множитель приглушения (считает выход «в микрофон», читают оба).
    pub(crate) duck_gain: AtomicF32,
}

/// Снимок параметров на один блок.
#[derive(Debug, Clone, Copy)]
struct Snapshot {
    speed: Option<f32>,
    reverb: Option<(f32, f32, f32)>,
    echo: Option<(f32, f32, f32)>,
    radio: bool,
    distortion: Option<f32>,
    bitcrush: Option<(f32, f32)>,
    robot: Option<f32>,
}

impl EffectsParams {
    pub fn apply(&self, s: &EffectsSettings) {
        let b = |a: &AtomicBool, v: bool| a.store(v, Ordering::Relaxed);
        b(&self.enabled, s.enabled);
        b(&self.on_monitor, s.on_monitor);
        b(&self.on_virtual, s.on_virtual);
        b(&self.speed_on, s.speed_on);
        self.speed.set(s.speed.clamp(0.25, 3.0));
        b(&self.reverb_on, s.reverb_on);
        self.reverb_room.set(s.reverb_room.clamp(0.0, 1.0));
        self.reverb_damp.set(s.reverb_damp.clamp(0.0, 1.0));
        self.reverb_mix.set(s.reverb_mix.clamp(0.0, 1.0));
        b(&self.echo_on, s.echo_on);
        self.echo_ms.set(s.echo_ms.clamp(20.0, 1900.0));
        self.echo_feedback.set(s.echo_feedback.clamp(0.0, 0.9));
        self.echo_mix.set(s.echo_mix.clamp(0.0, 1.0));
        b(&self.radio_on, s.radio_on);
        b(&self.distortion_on, s.distortion_on);
        self.distortion_drive.set(s.distortion_drive.clamp(1.0, 50.0));
        b(&self.bitcrush_on, s.bitcrush_on);
        self.bitcrush_bits.set(s.bitcrush_bits.clamp(2.0, 16.0));
        self.bitcrush_downsample.set(s.bitcrush_downsample.clamp(1.0, 32.0));
        b(&self.robot_on, s.robot_on);
        self.robot_hz.set(s.robot_hz.clamp(10.0, 500.0));
    }

    pub fn set_ducking(&self, on: bool, threshold: f32, level: f32) {
        let was_on = self.duck_on.swap(on, Ordering::Relaxed);
        if on && !was_on {
            self.duck_gain.set(1.0);
        }
        self.duck_threshold.set(threshold.clamp(0.001, 1.0));
        self.duck_level.set(level.clamp(0.0, 1.0));
        if !on {
            self.duck_gain.set(1.0);
        }
    }

    /// Множитель громкости звуков из-за приглушения.
    pub fn duck_gain(&self) -> f32 {
        if self.duck_on.load(Ordering::Relaxed) { self.duck_gain.get().clamp(0.0, 1.0) } else { 1.0 }
    }

    pub fn active_for(&self, kind: BusKind) -> bool {
        self.enabled.load(Ordering::Relaxed)
            && match kind {
                BusKind::Monitor => self.on_monitor.load(Ordering::Relaxed),
                BusKind::Virtual => self.on_virtual.load(Ordering::Relaxed),
            }
    }

    /// Множитель скорости для голосов этого выхода.
    pub fn speed_for(&self, kind: BusKind) -> f64 {
        if self.active_for(kind) && self.speed_on.load(Ordering::Relaxed) { self.speed.get() as f64 } else { 1.0 }
    }

    fn snapshot(&self) -> Snapshot {
        let on = |a: &AtomicBool| a.load(Ordering::Relaxed);
        Snapshot {
            speed: on(&self.speed_on).then(|| self.speed.get()),
            reverb: on(&self.reverb_on)
                .then(|| (self.reverb_room.get(), self.reverb_damp.get(), self.reverb_mix.get())),
            echo: on(&self.echo_on).then(|| (self.echo_ms.get(), self.echo_feedback.get(), self.echo_mix.get())),
            radio: on(&self.radio_on),
            distortion: on(&self.distortion_on).then(|| self.distortion_drive.get()),
            bitcrush: on(&self.bitcrush_on).then(|| (self.bitcrush_bits.get(), self.bitcrush_downsample.get())),
            robot: on(&self.robot_on).then(|| self.robot_hz.get()),
        }
    }
}

// ---------------- Строительные блоки ----------------

struct Comb {
    buf: Vec<f32>,
    idx: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], idx: 0, store: 0.0 }
    }
    #[inline]
    fn process(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.idx];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.idx] = input + self.store * feedback;
        self.idx = (self.idx + 1) % self.buf.len();
        out
    }
    fn clear(&mut self) {
        self.buf.fill(0.0);
        self.store = 0.0;
    }
}

struct Allpass {
    buf: Vec<f32>,
    idx: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], idx: 0 }
    }
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let b = self.buf[self.idx];
        self.buf[self.idx] = input + b * 0.5;
        self.idx = (self.idx + 1) % self.buf.len();
        b - input
    }
    fn clear(&mut self) {
        self.buf.fill(0.0);
    }
}

/// Реверберация по схеме Freeverb (8 гребенчатых + 4 всепропускающих фильтра на канал).
struct Reverb {
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<Allpass>; 2],
}

impl Reverb {
    fn new(rate: u32) -> Self {
        const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
        const SPREAD: usize = 23;
        let k = rate as f32 / 44_100.0;
        let scale = |n: usize| (n as f32 * k) as usize;
        let make = |extra: usize| -> (Vec<Comb>, Vec<Allpass>) {
            (
                COMBS.iter().map(|&n| Comb::new(scale(n + extra))).collect(),
                ALLPASSES.iter().map(|&n| Allpass::new(scale(n + extra))).collect(),
            )
        };
        let (cl, al) = make(0);
        let (cr, ar) = make(SPREAD);
        Self { combs: [cl, cr], allpasses: [al, ar] }
    }

    fn process(&mut self, buf: &mut [f32], room: f32, damp: f32, mix: f32) {
        let feedback = room * 0.28 + 0.7;
        let damp = damp * 0.4;
        for frame in buf.chunks_exact_mut(2) {
            let input = (frame[0] + frame[1]) * 0.015;
            for ch in 0..2 {
                let mut acc = 0.0;
                for c in &mut self.combs[ch] {
                    acc += c.process(input, feedback, damp);
                }
                for a in &mut self.allpasses[ch] {
                    acc = a.process(acc);
                }
                frame[ch] = frame[ch] * (1.0 - mix * 0.5) + acc * 3.0 * mix;
            }
        }
    }

    fn clear(&mut self) {
        self.combs.iter_mut().flatten().for_each(Comb::clear);
        self.allpasses.iter_mut().flatten().for_each(Allpass::clear);
    }
}

/// Эхо: линия задержки до 2 секунд.
struct Echo {
    buf: Vec<f32>,
    idx: usize,
    rate: u32,
}

impl Echo {
    fn new(rate: u32) -> Self {
        Self { buf: vec![0.0; rate as usize * 2 * 2], idx: 0, rate }
    }

    fn process(&mut self, buf: &mut [f32], ms: f32, feedback: f32, mix: f32) {
        let frames = self.buf.len() / 2;
        let delay = ((ms / 1000.0 * self.rate as f32) as usize).clamp(1, frames - 1);
        for frame in buf.chunks_exact_mut(2) {
            let read = (self.idx + frames - delay) % frames;
            for ch in 0..2 {
                let delayed = self.buf[read * 2 + ch];
                self.buf[self.idx * 2 + ch] = frame[ch] + delayed * feedback;
                frame[ch] += delayed * mix;
            }
            self.idx = (self.idx + 1) % frames;
        }
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
    }
}

/// Двухполюсный фильтр (biquad) — для «рации».
#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: [f32; 2],
    z2: [f32; 2],
}

impl Biquad {
    fn new(rate: u32, freq: f32, high_pass: bool) -> Self {
        let w = 2.0 * PI * freq / rate as f32;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * 0.707);
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = if high_pass {
            ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0)
        } else {
            ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0)
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            ..Default::default()
        }
    }

    #[inline]
    fn process(&mut self, x: f32, ch: usize) -> f32 {
        let y = self.b0 * x + self.z1[ch];
        self.z1[ch] = self.b1 * x - self.a1 * y + self.z2[ch];
        self.z2[ch] = self.b2 * x - self.a2 * y;
        y
    }

    fn clear(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }
}

// ---------------- Цепочка ----------------

pub(crate) struct EffectsChain {
    rate: u32,
    reverb: Reverb,
    echo: Echo,
    radio: [Biquad; 2],
    crush_hold: [f32; 2],
    crush_count: f32,
    robot_phase: f32,
    was_active: bool,
}

impl EffectsChain {
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            reverb: Reverb::new(rate),
            echo: Echo::new(rate),
            radio: [Biquad::new(rate, 450.0, true), Biquad::new(rate, 2800.0, false)],
            crush_hold: [0.0; 2],
            crush_count: 0.0,
            robot_phase: 0.0,
            was_active: false,
        }
    }

    fn clear(&mut self) {
        self.reverb.clear();
        self.echo.clear();
        self.radio.iter_mut().for_each(Biquad::clear);
        self.crush_hold = [0.0; 2];
        self.crush_count = 0.0;
    }

    /// Обрабатывает стерео-буфер на месте.
    pub fn process(&mut self, params: &EffectsParams, kind: BusKind, buf: &mut [f32]) {
        let active = params.active_for(kind);
        if !active {
            if self.was_active {
                self.clear();
                self.was_active = false;
            }
            return;
        }
        self.was_active = true;
        let p = params.snapshot();
        let _ = p.speed; // скорость применяется к голосам, не к буферу

        if let Some((bits, down)) = p.bitcrush {
            let levels = 2f32.powf(bits - 1.0);
            for frame in buf.chunks_exact_mut(2) {
                self.crush_count += 1.0;
                if self.crush_count >= down {
                    self.crush_count -= down;
                    for ch in 0..2 {
                        self.crush_hold[ch] = (frame[ch] * levels).round() / levels;
                    }
                }
                frame[0] = self.crush_hold[0];
                frame[1] = self.crush_hold[1];
            }
        }
        if let Some(drive) = p.distortion {
            let norm = drive.tanh();
            for s in buf.iter_mut() {
                *s = (*s * drive).tanh() / norm * 0.8;
            }
        }
        if let Some(hz) = p.robot {
            let step = 2.0 * PI * hz / self.rate as f32;
            for frame in buf.chunks_exact_mut(2) {
                let m = self.robot_phase.sin();
                frame[0] *= m * 1.4;
                frame[1] *= m * 1.4;
                self.robot_phase = (self.robot_phase + step) % (2.0 * PI);
            }
        }
        if p.radio {
            for frame in buf.chunks_exact_mut(2) {
                for ch in 0..2 {
                    let x = self.radio[0].process(frame[ch], ch);
                    let x = self.radio[1].process(x, ch);
                    frame[ch] = (x * 3.0).tanh() * 0.6;
                }
            }
        }
        if let Some((ms, fb, mix)) = p.echo {
            self.echo.process(buf, ms, fb, mix);
        }
        if let Some((room, damp, mix)) = p.reverb {
            self.reverb.process(buf, room, damp, mix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_do_not_blow_up() {
        let params = EffectsParams::default();
        let mut all = EffectsSettings { enabled: true, ..Default::default() };
        all.reverb_on = true;
        all.echo_on = true;
        all.radio_on = true;
        all.distortion_on = true;
        all.bitcrush_on = true;
        all.robot_on = true;
        params.apply(&all);
        let mut chain = EffectsChain::new(48_000);
        let mut buf: Vec<f32> = (0..4096).map(|i| ((i as f32) * 0.03).sin() * 0.5).collect();
        for _ in 0..50 {
            chain.process(&params, BusKind::Virtual, &mut buf);
            assert!(buf.iter().all(|s| s.is_finite() && s.abs() < 10.0));
        }
        // Выключенные эффекты не трогают звук.
        params.apply(&EffectsSettings::default());
        let mut clean = vec![0.25f32; 64];
        chain.process(&params, BusKind::Virtual, &mut clean);
        assert!(clean.iter().all(|&s| s == 0.25));
    }

    #[test]
    fn speed_only_when_enabled() {
        let params = EffectsParams::default();
        params.apply(&EffectsSettings {
            enabled: true,
            speed_on: true,
            speed: 1.5,
            on_monitor: false,
            ..Default::default()
        });
        assert_eq!(params.speed_for(BusKind::Virtual), 1.5);
        assert_eq!(params.speed_for(BusKind::Monitor), 1.0);
    }
}
