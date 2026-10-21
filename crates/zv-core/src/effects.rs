//! Настройки эффектов. Сами эффекты — в звуковом движке (zv-audio).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffectsSettings {
    /// Общий выключатель.
    pub enabled: bool,
    /// Применять к звуку «себе».
    pub on_monitor: bool,
    /// Применять к звуку «в микрофон».
    pub on_virtual: bool,

    /// Скорость воспроизведения (меняет и тон): 1.0 — как есть.
    pub speed_on: bool,
    pub speed: f32,

    pub reverb_on: bool,
    /// Размер комнаты 0..1.
    pub reverb_room: f32,
    /// Глушение высоких 0..1.
    pub reverb_damp: f32,
    /// Доля эффекта 0..1.
    pub reverb_mix: f32,

    pub echo_on: bool,
    pub echo_ms: f32,
    pub echo_feedback: f32,
    pub echo_mix: f32,

    /// «Рация»: узкая полоса частот и лёгкий перегруз.
    pub radio_on: bool,

    pub distortion_on: bool,
    /// Сила перегруза 1..30.
    pub distortion_drive: f32,

    /// «8 бит»: меньше разрядов и частоты.
    pub bitcrush_on: bool,
    pub bitcrush_bits: f32,
    /// Во сколько раз понизить частоту 1..20.
    pub bitcrush_downsample: f32,

    /// «Робот»: кольцевая модуляция.
    pub robot_on: bool,
    pub robot_hz: f32,
}

impl Default for EffectsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            on_monitor: true,
            on_virtual: true,
            speed_on: false,
            speed: 1.0,
            reverb_on: false,
            reverb_room: 0.6,
            reverb_damp: 0.4,
            reverb_mix: 0.3,
            echo_on: false,
            echo_ms: 300.0,
            echo_feedback: 0.35,
            echo_mix: 0.35,
            radio_on: false,
            distortion_on: false,
            distortion_drive: 6.0,
            bitcrush_on: false,
            bitcrush_bits: 8.0,
            bitcrush_downsample: 4.0,
            robot_on: false,
            robot_hz: 60.0,
        }
    }
}

/// Быстрые пресеты эффектов.
pub fn effect_presets() -> Vec<(&'static str, EffectsSettings)> {
    let off = EffectsSettings { enabled: true, ..Default::default() };
    vec![
        (
            "Пещера",
            EffectsSettings { reverb_on: true, reverb_room: 0.9, reverb_damp: 0.2, reverb_mix: 0.45, ..off.clone() },
        ),
        (
            "Стадион",
            EffectsSettings {
                echo_on: true,
                echo_ms: 420.0,
                echo_feedback: 0.4,
                echo_mix: 0.3,
                reverb_on: true,
                reverb_room: 0.8,
                ..off.clone()
            },
        ),
        ("Рация", EffectsSettings { radio_on: true, ..off.clone() }),
        ("Бурундук", EffectsSettings { speed_on: true, speed: 1.6, ..off.clone() }),
        ("Великан", EffectsSettings { speed_on: true, speed: 0.7, reverb_on: true, reverb_mix: 0.2, ..off.clone() }),
        (
            "Робот",
            EffectsSettings {
                robot_on: true,
                robot_hz: 50.0,
                bitcrush_on: true,
                bitcrush_bits: 10.0,
                bitcrush_downsample: 1.0,
                ..off.clone()
            },
        ),
        (
            "Ретро-игра",
            EffectsSettings { bitcrush_on: true, bitcrush_bits: 6.0, bitcrush_downsample: 6.0, ..off.clone() },
        ),
        (
            "Громкоговоритель",
            EffectsSettings {
                radio_on: true,
                distortion_on: true,
                distortion_drive: 4.0,
                echo_on: true,
                echo_ms: 180.0,
                echo_feedback: 0.2,
                echo_mix: 0.2,
                ..off
            },
        ),
    ]
}
