//! Настройки программы. Хранятся в `%APPDATA%\Zvukopad\settings.json`.
//!
//! Все поля имеют значения по умолчанию (`#[serde(default)]`), поэтому старый файл настроек
//! без новых полей читается без ошибок — это важно для обновлений.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Куда звучит звук.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlayTarget {
    /// Только в наушники/колонки.
    Monitor,
    /// Только в виртуальный микрофон (Discord/Skype).
    Virtual,
    /// И туда, и туда.
    #[default]
    Both,
}

impl PlayTarget {
    /// Следующий режим по кругу: оба → себе → в микрофон → оба.
    pub fn next(self) -> Self {
        match self {
            PlayTarget::Both => PlayTarget::Monitor,
            PlayTarget::Monitor => PlayTarget::Virtual,
            PlayTarget::Virtual => PlayTarget::Both,
        }
    }
    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            PlayTarget::Monitor => "Себе",
            PlayTarget::Virtual => "В микрофон",
            PlayTarget::Both => "Себе и в микрофон",
        })
    }
    pub fn monitor(self) -> bool {
        matches!(self, PlayTarget::Monitor | PlayTarget::Both)
    }
    pub fn virtual_out(self) -> bool {
        matches!(self, PlayTarget::Virtual | PlayTarget::Both)
    }
}

/// Что делать при повторном нажатии на уже играющий звук.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlayMode {
    /// Начать заново.
    #[default]
    Restart,
    /// Запустить ещё одну копию поверх.
    Overlap,
    /// Остановить.
    Toggle,
}

impl PlayMode {
    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            PlayMode::Restart => "Начать заново",
            PlayMode::Overlap => "Наложить поверх",
            PlayMode::Toggle => "Остановить",
        })
    }
}

/// Ссылка на аудиоустройство. `id` стабилен между запусками, `name` — запасной вариант
/// (и для показа, если устройство сейчас отключено).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeviceRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// Выход «себе». `None` — устройство Windows по умолчанию.
    pub monitor_device: Option<DeviceRef>,
    /// Виртуальный выход (например, «CABLE Input»). `None` — не используется.
    pub virtual_device: Option<DeviceRef>,
    /// Микрофон, который подмешивается в виртуальный выход, чтобы вас было слышно вместе со звуками.
    pub mic_device: Option<DeviceRef>,
    pub mic_passthrough: bool,
    /// Громкости в линейном масштабе, 1.0 = 100 %.
    pub monitor_volume: f32,
    pub virtual_volume: f32,
    pub mic_volume: f32,
    /// Звук «себе» выключен (клавишей «Без звука»).
    pub monitor_muted: bool,
    pub target: PlayTarget,
    pub play_mode: PlayMode,
    /// Выравнивать громкость звуков.
    pub normalize: bool,
    /// Приглушать звуки, пока вы говорите (нужно «Подмешивать мой голос»).
    pub duck_enabled: bool,
    /// Порог уровня микрофона, 0..1.
    pub duck_threshold: f32,
    /// Громкость звуков, пока вы говорите, 0..1.
    pub duck_level: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            monitor_device: None,
            virtual_device: None,
            mic_device: None,
            mic_passthrough: true,
            monitor_volume: 0.8,
            virtual_volume: 1.0,
            mic_volume: 1.0,
            monitor_muted: false,
            target: PlayTarget::Both,
            play_mode: PlayMode::Restart,
            normalize: true,
            duck_enabled: false,
            duck_threshold: 0.08,
            duck_level: 0.35,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordingSettings {
    /// Источник записи. `loopback = true` — записывается то, что звучит в этом устройстве вывода.
    pub source: Option<DeviceRef>,
    pub loopback: bool,
    /// Подпапка внутри папки звуков, куда кладутся записи.
    pub folder: String,
}

impl Default for RecordingSettings {
    fn default() -> Self {
        Self { source: None, loopback: true, folder: crate::i18n::tr("Записи").into() }
    }
}

// ===================== Оверлей =====================

/// Где на экране оверлей.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OverlayPosition {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    Center,
}

impl OverlayPosition {
    pub const ALL: [OverlayPosition; 6] = [
        OverlayPosition::TopLeft,
        OverlayPosition::TopCenter,
        OverlayPosition::TopRight,
        OverlayPosition::Center,
        OverlayPosition::BottomLeft,
        OverlayPosition::BottomRight,
    ];
    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            OverlayPosition::TopLeft => "Слева сверху",
            OverlayPosition::TopRight => "Справа сверху",
            OverlayPosition::BottomLeft => "Слева снизу",
            OverlayPosition::BottomRight => "Справа снизу",
            OverlayPosition::TopCenter => "Сверху по центру",
            OverlayPosition::Center => "По центру экрана",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OverlayLayout {
    /// Список строк «клавиша — звук».
    #[default]
    List,
    /// Плитки, окрашенные в цвет звука.
    Grid,
}

impl OverlayLayout {
    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            OverlayLayout::List => "Список",
            OverlayLayout::Grid => "Плитки",
        })
    }
}

/// Когда оверлей виден.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OverlayMode {
    /// Никогда (кроме подсказок нумпада, если они включены).
    Off,
    /// Всегда.
    Always,
    /// Включается и выключается клавишей.
    #[default]
    Toggle,
    /// Появляется по клавише на несколько секунд и сам исчезает.
    Peek,
}

impl OverlayMode {
    pub const ALL: [OverlayMode; 4] = [OverlayMode::Off, OverlayMode::Toggle, OverlayMode::Peek, OverlayMode::Always];
    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            OverlayMode::Off => "Выключен",
            OverlayMode::Always => "Всегда виден",
            OverlayMode::Toggle => "Вкл/выкл клавишей",
            OverlayMode::Peek => "По клавише на время",
        })
    }
}

/// Внешний вид оверлея — то, что сохраняется в пресеты.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayStyle {
    pub position: OverlayPosition,
    pub layout: OverlayLayout,
    /// Ширина: доля ширины экрана (0.1..1.0).
    pub width: f32,
    /// Непрозрачность фона, 0..1.
    pub opacity: f32,
    pub font_size: f32,
    /// Сколько звуков показывать.
    pub max_items: usize,
    /// Колонок в режиме плиток.
    pub columns: usize,
    /// Только звуки с горячими клавишами (иначе — все с номерами).
    pub only_with_hotkeys: bool,
    /// Показывать номера нумпада.
    pub show_codes: bool,
}

impl Default for OverlayStyle {
    fn default() -> Self {
        Self {
            position: OverlayPosition::TopRight,
            layout: OverlayLayout::List,
            width: 0.2,
            opacity: 0.75,
            font_size: 14.0,
            max_items: 20,
            columns: 4,
            only_with_hotkeys: true,
            show_codes: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayPreset {
    pub name: String,
    pub style: OverlayStyle,
}

/// Встроенные пресеты оверлея.
pub fn builtin_overlay_presets() -> Vec<OverlayPreset> {
    let d = OverlayStyle::default();
    vec![
        OverlayPreset { name: "Угол справа".into(), style: d.clone() },
        OverlayPreset {
            name: "Угол слева".into(),
            style: OverlayStyle { position: OverlayPosition::TopLeft, ..d.clone() },
        },
        OverlayPreset {
            name: "Полоса сверху".into(),
            style: OverlayStyle {
                position: OverlayPosition::TopCenter,
                layout: OverlayLayout::Grid,
                width: 0.6,
                columns: 6,
                max_items: 12,
                font_size: 13.0,
                ..d.clone()
            },
        },
        OverlayPreset {
            name: "Во весь экран (плитки)".into(),
            style: OverlayStyle {
                position: OverlayPosition::Center,
                layout: OverlayLayout::Grid,
                width: 0.8,
                opacity: 0.85,
                font_size: 18.0,
                max_items: 40,
                columns: 5,
                only_with_hotkeys: false,
                ..d.clone()
            },
        },
        OverlayPreset {
            name: "Компактный".into(),
            style: OverlayStyle { width: 0.14, font_size: 12.0, max_items: 10, opacity: 0.55, ..d },
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlaySettings {
    pub mode: OverlayMode,
    /// Включён ли (для режима «Вкл/выкл клавишей»).
    pub visible: bool,
    /// Сколько секунд держать в режиме «По клавише на время».
    pub peek_secs: f32,
    /// Показывать на время после запуска звука горячей клавишей.
    pub show_on_play: bool,
    /// Показывать подсказки, пока набирается номер на нумпаде (в любом режиме).
    pub show_on_numpad: bool,
    pub style: OverlayStyle,
    /// Пресеты пользователя.
    pub presets: Vec<OverlayPreset>,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            mode: OverlayMode::Toggle,
            visible: false,
            peek_secs: 3.0,
            show_on_play: false,
            show_on_numpad: true,
            style: OverlayStyle::default(),
            presets: Vec::new(),
        }
    }
}

// ===================== Нумпад =====================

/// Модификатор, который держат при наборе номера.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NumpadModifier {
    #[default]
    Alt,
    Ctrl,
    Shift,
    CtrlAlt,
}

impl NumpadModifier {
    pub const ALL: [NumpadModifier; 4] =
        [NumpadModifier::Alt, NumpadModifier::Ctrl, NumpadModifier::Shift, NumpadModifier::CtrlAlt];
    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self {
            NumpadModifier::Alt => "Alt",
            NumpadModifier::Ctrl => "Ctrl",
            NumpadModifier::Shift => "Shift",
            NumpadModifier::CtrlAlt => "Ctrl+Alt",
        })
    }
    /// Префикс для строки горячей клавиши.
    pub fn prefix(self) -> &'static str {
        match self {
            NumpadModifier::Alt => "Alt+",
            NumpadModifier::Ctrl => "Ctrl+",
            NumpadModifier::Shift => "Shift+",
            NumpadModifier::CtrlAlt => "Ctrl+Alt+",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NumpadSettings {
    /// Набор номера звука: зажать модификатор, набрать цифры на нумпаде, отпустить.
    pub enabled: bool,
    pub modifier: NumpadModifier,
    /// Сколько подсказок показывать в оверлее.
    pub max_hints: usize,
    /// Сам выдавать номера новым звукам.
    pub auto_assign: bool,
}

impl Default for NumpadSettings {
    fn default() -> Self {
        Self { enabled: true, modifier: NumpadModifier::Alt, max_hints: 8, auto_assign: true }
    }
}

/// Буфер «что сейчас было»: последние секунды источника в памяти.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReplaySettings {
    /// Включать буфер при запуске программы.
    pub enabled: bool,
    pub source: Option<DeviceRef>,
    pub loopback: bool,
    /// Сколько секунд держать в памяти.
    pub secs: u32,
    /// Подпапка внутри папки звуков.
    pub folder: String,
}

impl Default for ReplaySettings {
    fn default() -> Self {
        Self {
            enabled: false, source: None, loopback: true, secs: 30, folder: crate::i18n::tr("Повторы").into()
        }
    }
}

/// Автонажатие клавиши рации (push-to-talk), пока звук играет «в микрофон».
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoPttSettings {
    pub enabled: bool,
    /// Клавиша рации, как в игре/Discord (`V`, `CapsLock`, `F13`…).
    pub key: String,
    /// Сколько ещё держать клавишу после конца звука, мс.
    pub release_ms: u32,
}

impl Default for AutoPttSettings {
    fn default() -> Self {
        Self { enabled: false, key: String::new(), release_ms: 300 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TtsSettings {
    pub plugin_id: String,
    pub voice_id: String,
    /// Скорость речи: -10..10.
    pub rate: i32,
    pub folder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub tags_panel_open: bool,
    pub queue_panel_open: bool,
    /// Фильтр по тегам: все выбранные (true) или любой из них (false).
    pub tags_match_all: bool,
    /// Язык интерфейса: `auto` — как в Windows, или код (`ru`, `en`, …).
    pub language: String,
    /// Скрывать пути к папкам (в них имя пользователя Windows) — для стримов и скриншотов.
    pub hide_paths: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            tags_panel_open: true,
            queue_panel_open: false,
            tags_match_all: false,
            language: "auto".into(),
            hide_paths: false,
        }
    }
}

/// Старый формат горячих клавиш (версия 0.1) — только для переноса настроек.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LegacyHotkeys {
    pub stop_all: String,
    pub toggle_overlay: String,
    pub toggle_recording: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Своя папка звуков. `None` — `Документы\Zvukopad\Sounds`.
    pub sounds_dir: Option<PathBuf>,
    pub audio: AudioSettings,
    pub recording: RecordingSettings,
    pub replay: ReplaySettings,
    pub auto_ptt: AutoPttSettings,
    pub overlay: OverlaySettings,
    pub controls: crate::controls::Controls,
    pub numpad: NumpadSettings,
    pub effects: crate::effects::EffectsSettings,
    pub timers: Vec<crate::timers::TimerTask>,
    pub tts: TtsSettings,
    pub ui: UiSettings,
    /// Из версии 0.1; переносится в `controls` при загрузке.
    #[serde(skip_serializing)]
    pub hotkeys: Option<LegacyHotkeys>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sounds_dir: None,
            audio: AudioSettings::default(),
            recording: RecordingSettings::default(),
            replay: ReplaySettings::default(),
            auto_ptt: AutoPttSettings::default(),
            overlay: OverlaySettings::default(),
            controls: crate::controls::Controls::default(),
            numpad: NumpadSettings::default(),
            effects: crate::effects::EffectsSettings::default(),
            timers: Vec::new(),
            tts: TtsSettings { folder: crate::i18n::tr("Синтез речи").into(), ..Default::default() },
            ui: UiSettings::default(),
            hotkeys: None,
        }
    }
}

impl Settings {
    /// Переносит настройки из старых версий.
    pub fn migrate(&mut self) {
        use crate::controls::Action;
        if let Some(old) = self.hotkeys.take() {
            for (action, key) in [
                (Action::StopAll, old.stop_all),
                (Action::ToggleOverlay, old.toggle_overlay),
                (Action::ToggleRecording, old.toggle_recording),
            ] {
                if !key.is_empty() {
                    self.controls.bindings.insert(action, key);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::Action;

    #[test]
    fn old_file_without_new_fields_still_loads() {
        let s: Settings = serde_json::from_str(r#"{"audio":{"monitor_volume":0.5}}"#).unwrap();
        assert_eq!(s.audio.monitor_volume, 0.5);
        assert_eq!(s.audio.virtual_volume, 1.0);
        assert_eq!(s.controls.key(Action::StopAll), "Ctrl+Alt+S");
    }

    #[test]
    fn v01_hotkeys_are_migrated() {
        let mut s: Settings =
            serde_json::from_str(r#"{"hotkeys":{"stop_all":"F9","toggle_overlay":"","toggle_recording":"F10"},"overlay":{"visible":true,"corner":"TopLeft"}}"#)
                .unwrap();
        s.migrate();
        assert_eq!(s.controls.key(Action::StopAll), "F9");
        assert_eq!(s.controls.key(Action::ToggleRecording), "F10");
        assert!(s.overlay.visible);
        let text = serde_json::to_string(&s).unwrap();
        assert!(!text.contains("\"hotkeys\""));
    }
}
