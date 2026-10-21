//! Схема управления: какие клавиши (или кнопки мыши) выполняют общие действия.
//!
//! Клавиши звуков хранятся в библиотеке (у каждого звука своя), а здесь — действия
//! вроде «остановить всё», «громче», «следующий в очереди». Есть готовые пресеты,
//! после правки пресет становится «Свой».

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::settings::NumpadModifier;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Action {
    StopAll,
    PauseResume,
    RepeatLast,
    PlayRandom,
    ModeCycle,
    ModeBoth,
    ModeMonitor,
    ModeVirtual,
    MonitorVolumeUp,
    MonitorVolumeDown,
    MonitorMute,
    VirtualVolumeUp,
    VirtualVolumeDown,
    ToggleOverlay,
    PeekOverlay,
    ToggleRecording,
    ToggleEffects,
    QueuePlayStop,
    QueueNext,
    /// Включить/выключить клавиши звуков. Работает, даже когда они выключены.
    ToggleSoundHotkeys,
    /// Сохранить «что сейчас было» из буфера повтора.
    SaveReplay,
    /// Включить/выключить буфер повтора.
    ToggleReplay,
}

/// Группа действий — для удобного вывода в настройках.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionGroup {
    Playback,
    Mode,
    Volume,
    Other,
}

impl ActionGroup {
    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            ActionGroup::Playback => "Воспроизведение",
            ActionGroup::Mode => "Куда играть",
            ActionGroup::Volume => "Громкость",
            ActionGroup::Other => "Прочее",
        })
    }
}

impl Action {
    pub const ALL: [Action; 22] = [
        Action::StopAll,
        Action::PauseResume,
        Action::RepeatLast,
        Action::PlayRandom,
        Action::QueuePlayStop,
        Action::QueueNext,
        Action::ModeCycle,
        Action::ModeBoth,
        Action::ModeMonitor,
        Action::ModeVirtual,
        Action::MonitorVolumeUp,
        Action::MonitorVolumeDown,
        Action::MonitorMute,
        Action::VirtualVolumeUp,
        Action::VirtualVolumeDown,
        Action::ToggleOverlay,
        Action::PeekOverlay,
        Action::ToggleRecording,
        Action::ToggleEffects,
        Action::SaveReplay,
        Action::ToggleReplay,
        Action::ToggleSoundHotkeys,
    ];

    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            Action::StopAll => "Остановить все звуки",
            Action::PauseResume => "Пауза / продолжить",
            Action::RepeatLast => "Повторить последний звук",
            Action::PlayRandom => "Случайный звук",
            Action::ModeCycle => "Переключить по кругу",
            Action::ModeBoth => "Себе и в микрофон",
            Action::ModeMonitor => "Только себе",
            Action::ModeVirtual => "Только в микрофон",
            Action::MonitorVolumeUp => "Громче «себе»",
            Action::MonitorVolumeDown => "Тише «себе»",
            Action::MonitorMute => "Без звука «себе» (вкл/выкл)",
            Action::VirtualVolumeUp => "Громче «в микрофон»",
            Action::VirtualVolumeDown => "Тише «в микрофон»",
            Action::ToggleOverlay => "Оверлей: показать/скрыть",
            Action::PeekOverlay => "Оверлей: показать на время",
            Action::ToggleRecording => "Запись: начать/остановить",
            Action::ToggleEffects => "Эффекты: вкл/выкл",
            Action::QueuePlayStop => "Очередь: играть/стоп",
            Action::QueueNext => "Очередь: следующий",
            Action::ToggleSoundHotkeys => "Клавиши звуков: вкл/выкл",
            Action::SaveReplay => "Сохранить «что сейчас было»",
            Action::ToggleReplay => "Буфер повтора: вкл/выкл",
        })
    }

    pub fn group(self) -> ActionGroup {
        match self {
            Action::StopAll
            | Action::PauseResume
            | Action::RepeatLast
            | Action::PlayRandom
            | Action::QueuePlayStop
            | Action::QueueNext => ActionGroup::Playback,
            Action::ModeCycle | Action::ModeBoth | Action::ModeMonitor | Action::ModeVirtual => ActionGroup::Mode,
            Action::MonitorVolumeUp
            | Action::MonitorVolumeDown
            | Action::MonitorMute
            | Action::VirtualVolumeUp
            | Action::VirtualVolumeDown => ActionGroup::Volume,
            _ => ActionGroup::Other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Controls {
    /// Название применённого пресета или «Свой».
    pub preset: String,
    /// Действие → клавиша (`Ctrl+Alt+S`, `F13`, `Mouse4`, `Ctrl+MouseMiddle`).
    pub bindings: BTreeMap<Action, String>,
    /// Не «съедать» нажатия клавиш звуков — игра тоже их получит.
    pub pass_through: bool,
    /// Клавиши звуков включены.
    pub sound_hotkeys_enabled: bool,
    /// Шаг громкости для клавиш «громче/тише», доля (0.1 = 10 %).
    pub volume_step: f32,
    /// Enter в списке звуков играет выбранный звук.
    pub enter_plays: bool,
    /// Тег → клавиша «случайный звук из этого тега».
    pub tag_random: BTreeMap<String, String>,
}

impl Default for Controls {
    fn default() -> Self {
        let p = &presets()[0];
        Self {
            preset: p.name.to_string(),
            bindings: p.bindings(),
            pass_through: false,
            sound_hotkeys_enabled: true,
            volume_step: 0.1,
            enter_plays: true,
            tag_random: BTreeMap::new(),
        }
    }
}

impl Controls {
    pub fn key(&self, a: Action) -> &str {
        self.bindings.get(&a).map(String::as_str).unwrap_or("")
    }

    pub fn set_key(&mut self, a: Action, key: String) {
        if key.is_empty() {
            self.bindings.remove(&a);
        } else {
            self.bindings.insert(a, key);
        }
        self.preset = CUSTOM_PRESET.to_string();
    }

    /// Клавиша «случайный из тега».
    pub fn set_tag_key(&mut self, tag: &str, key: String) {
        if key.is_empty() {
            self.tag_random.remove(tag);
        } else {
            self.tag_random.insert(tag.to_string(), key);
        }
    }

    /// Какой тег занимает клавишу «случайный из тега».
    pub fn tag_owner(&self, key: &str) -> Option<&str> {
        self.tag_random.iter().find(|(_, k)| k.eq_ignore_ascii_case(key)).map(|(t, _)| t.as_str())
    }

    /// Какое действие уже занимает клавишу.
    pub fn owner(&self, key: &str) -> Option<Action> {
        self.bindings.iter().find(|(_, k)| k.eq_ignore_ascii_case(key)).map(|(a, _)| *a)
    }
}

pub const CUSTOM_PRESET: &str = "Свой";

/// Готовая схема управления.
pub struct ControlPreset {
    pub name: &'static str,
    pub description: &'static str,
    keys: &'static [(Action, &'static str)],
    pub numpad: Option<NumpadModifier>,
    pub pass_through: bool,
}

impl ControlPreset {
    pub fn bindings(&self) -> BTreeMap<Action, String> {
        self.keys.iter().map(|(a, k)| (*a, k.to_string())).collect()
    }
}

pub fn presets() -> &'static [ControlPreset] {
    &[
        ControlPreset {
            name: crate::APP_TITLE,
            description: "Всё на Ctrl+Alt+буква, номера звуков — Alt+цифры на нумпаде.",
            keys: &[
                (Action::StopAll, "Ctrl+Alt+S"),
                (Action::PauseResume, "Ctrl+Alt+P"),
                (Action::RepeatLast, "Ctrl+Alt+Space"),
                (Action::PlayRandom, "Ctrl+Alt+X"),
                (Action::ModeCycle, "Ctrl+Alt+M"),
                (Action::MonitorVolumeUp, "Ctrl+Alt+Up"),
                (Action::MonitorVolumeDown, "Ctrl+Alt+Down"),
                (Action::ToggleOverlay, "Ctrl+Alt+O"),
                (Action::PeekOverlay, "Ctrl+Alt+I"),
                (Action::ToggleRecording, "Ctrl+Alt+R"),
                (Action::SaveReplay, "Ctrl+Alt+B"),
                (Action::ToggleEffects, "Ctrl+Alt+E"),
                (Action::QueuePlayStop, "Ctrl+Alt+Q"),
                (Action::QueueNext, "Ctrl+Alt+N"),
                (Action::ToggleSoundHotkeys, "Ctrl+Alt+H"),
            ],
            numpad: Some(NumpadModifier::Alt),
            pass_through: false,
        },
        ControlPreset {
            name: "Как в Soundpad",
            description: "Номера звуков — Alt+цифры на нумпаде, как Numcode в Soundpad. Общие клавиши, \
                          как и в Soundpad, не назначены — задайте нужные сами.",
            keys: &[],
            numpad: Some(NumpadModifier::Alt),
            pass_through: false,
        },
        ControlPreset {
            name: "Мышь",
            description: "Боковые кнопки мыши: Mouse4 — повтор, Mouse5 — стоп, Ctrl+колесо — оверлей на время.",
            keys: &[
                (Action::RepeatLast, "Mouse4"),
                (Action::StopAll, "Mouse5"),
                (Action::PeekOverlay, "Ctrl+MouseMiddle"),
                (Action::ModeCycle, "Ctrl+Mouse4"),
                (Action::QueueNext, "Ctrl+Mouse5"),
            ],
            numpad: Some(NumpadModifier::Alt),
            pass_through: false,
        },
        ControlPreset {
            name: "Минимальный",
            description: "Только «остановить всё». Клавиши звуков назначаете сами.",
            keys: &[(Action::StopAll, "Ctrl+Alt+S")],
            numpad: None,
            pass_through: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_and_custom() {
        let mut c = Controls::default();
        assert_eq!(c.preset, crate::APP_TITLE);
        c.set_key(Action::StopAll, "F9".into());
        assert_eq!(c.preset, CUSTOM_PRESET);
        let text = serde_json::to_string(&c).unwrap();
        let back: Controls = serde_json::from_str(&text).unwrap();
        assert_eq!(back.key(Action::StopAll), "F9");
        assert_eq!(back.owner("f9"), Some(Action::StopAll));
    }
}
