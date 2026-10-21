//! Главное окно.
//!
//! Состояние программы живёт в [`ZvApp`]. Вкладки вынесены в отдельные файлы
//! и получают `&mut ZvApp`. Всё тяжёлое (звук, декодирование, плагины) работает
//! в своих потоках — окно только отправляет команды и показывает состояние.

mod controls_tab;
mod editor;
mod effects_tab;
mod help;
mod log_tab;
mod overlay;
pub mod pattern_tab;
mod record;
mod settings_tab;
mod sounds;
pub mod style;
mod timers_tab;
mod tts_tab;
mod widgets;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use zv_core::i18n::tr;
use zv_core::tf;

use eframe::egui::{self, RichText};
use zv_audio::{DeviceList, Engine, EngineConfig, EngineStatus, Levels, RecordingResult};
use zv_core::controls::Action;
use zv_core::library::{LibraryMeta, SoundEntry};
use zv_core::paths::AppPaths;
use zv_core::settings::{OverlayMode, PlayTarget, Settings};
use zv_core::timers::{TimerKind, now_unix};
use zv_plugins::TtsVoice;

use crate::hotkeys::{Bind, Dispatch, HotkeyService};
use crate::numpad::Numpad;
use crate::player::{PlayConfig, Player, SoundSlot, sound_key};
use crate::ptt::AutoPtt;
use crate::queue::Queue;
use crate::sysutil;
use crate::tts::{SynthPurpose, TtsEvent, TtsService};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Sounds,
    Record,
    Tts,
    Effects,
    Pattern,
    Timers,
    Settings,
    Log,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPage {
    Devices,
    Controls,
    Overlay,
    Folders,
    Language,
}

/// Куда назначается клавиша, пока пользователь её нажимает.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureTarget {
    Sound(String),
    Action(Action),
    /// Клавиша «случайный звук из тега».
    TagRandom(String),
}

/// События из горячих клавиш, которые обрабатывает окно.
enum UiEvent {
    Action(Action),
}

/// Пики волны для карточки звука: (id, число столбиков, пики).
type WaveformCache = (String, usize, Vec<(f32, f32)>);

struct Toast {
    text: String,
    error: bool,
    at: Instant,
}

pub struct ZvApp {
    pub paths: AppPaths,
    pub settings: Settings,
    settings_dirty: Option<Instant>,
    pub sounds_dir: PathBuf,
    pub entries: Vec<SoundEntry>,
    pub meta: LibraryMeta,
    meta_dirty: Option<Instant>,
    hotkeys_dirty: bool,
    pub player: Arc<Player>,
    pub engine: Engine,
    pub ptt: AutoPtt,
    pub hotkeys: HotkeyService,
    pub tts: TtsService,
    pub piper: crate::piper::PiperPaths,
    pub downloads: crate::download::Downloader,
    pub voice_url_buf: String,
    pub devices: DeviceList,
    pub status: EngineStatus,
    last_engine_config: Option<EngineConfig>,
    pub tab: Tab,
    pub search: String,
    pub category: Option<String>,
    pub selected: Option<String>,
    pub rename_buf: String,
    pub manual_hotkey_buf: String,
    pub capture: Option<CaptureTarget>,
    pub pending_delete: Option<String>,
    pub sounds_dir_buf: String,
    toasts: Vec<Toast>,
    pub levels: Levels,
    pub playing: HashSet<u64>,
    pub tts_text: String,
    pub tts_voices: HashMap<String, Result<Vec<TtsVoice>, String>>,
    tts_requested: HashSet<String>,
    tts_counter: u64,
    pub log_only_problems: bool,
    /// Переименование прямо в списке: (id, текст, нужно поставить фокус).
    pub inline_rename: Option<(String, String, bool)>,
    pub editor: Option<editor::EditorState>,
    repaint_ctx: egui::Context,
    pub numpad: Arc<Numpad>,
    pub queue: Arc<Queue>,
    ui_rx: crossbeam_channel::Receiver<UiEvent>,
    played_rx: crossbeam_channel::Receiver<String>,
    pub settings_page: SettingsPage,
    /// Фильтр по тегам.
    pub selected_tags: BTreeSet<String>,
    pub tag_input: String,
    /// Переименование тега: (старое имя, новое).
    pub tag_rename: Option<(String, String)>,
    /// До какого момента показывать оверлей (режим «на время», показ при запуске звука).
    pub overlay_peek_until: Option<Instant>,
    pub overlay_preset_name: String,
    /// Запись по таймеру: когда остановить (секунды Unix).
    pub timer_record_until: Option<u64>,
    pub timer_form: timers_tab::TimerForm,
    pub focus_search: bool,
    /// Волна для карточки звука: (id, число столбиков, данные).
    waveform_cache: Option<WaveformCache>,
    /// Номер звука, который сейчас вводят: (id, текст).
    pub code_edit: Option<(String, String)>,
    /// Номер уже занят: (id, номер, у кого занят) — ждёт ответа «перезаписать / отмена».
    pub code_conflict: Option<(String, u32, String)>,
    /// Редактор паттернов.
    pub pattern_ed: pattern_tab::PatternEditor,
}

impl ZvApp {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: AppPaths, settings: Settings) -> Self {
        style::setup(&cc.egui_ctx);

        let sounds_dir = settings.sounds_dir.clone().unwrap_or_else(|| paths.default_sounds_dir.clone());
        paths.ensure_dirs(&sounds_dir);
        let meta: LibraryMeta = zv_core::load_json_or_default(&paths.library_file());

        let engine = Engine::start(engine_config(&settings));
        let player = Player::new(engine.clone(), play_config(&settings));
        let ctx = cc.egui_ctx.clone();
        player.set_repaint(move || ctx.request_repaint());

        let (played_tx, played_rx) = crossbeam_channel::bounded(64);
        player.set_played_listener(played_tx);
        let ctx = cc.egui_ctx.clone();
        let numpad = Numpad::new(player.clone(), move || ctx.request_repaint());
        let ctx = cc.egui_ctx.clone();
        let queue = Queue::new(player.clone(), move || ctx.request_repaint());

        // Что делают горячие клавиши. Звук запускается прямо отсюда, не дожидаясь окна.
        let (ui_tx, ui_rx) = crossbeam_channel::unbounded();
        let dispatch: Dispatch = {
            let (player, numpad, queue, ctx) = (player.clone(), numpad.clone(), queue.clone(), cc.egui_ctx.clone());
            Arc::new(move |bind: Bind| {
                match bind {
                    Bind::Sound(id) => player.play(&id, None),
                    Bind::NumDigit(d) => numpad.digit(d),
                    Bind::NumCancel => numpad.cancel(),
                    Bind::RandomSet(tag, ids) => player.play_random_from(&ids, &tf!("В теге «{}» нет звуков", tag)),
                    Bind::Action(a) => match a {
                        Action::StopAll => {
                            queue.stop();
                            player.stop_all();
                        }
                        Action::RepeatLast => player.repeat_last(),
                        Action::PlayRandom => player.play_random(),
                        Action::PauseResume => {
                            player.toggle_pause();
                        }
                        Action::QueuePlayStop => queue.toggle(),
                        Action::QueueNext => queue.next(),
                        other => {
                            let _ = ui_tx.send(UiEvent::Action(other));
                        }
                    },
                }
                ctx.request_repaint();
            })
        };
        let hotkeys = HotkeyService::new(dispatch);

        let ctx = cc.egui_ctx.clone();
        let plugin_dirs = vec![sysutil::builtin_plugins_dir(), paths.user_plugins_dir()];
        let piper_paths = crate::piper::PiperPaths::new(&paths.local_dir);
        let tts = TtsService::new(plugin_dirs, piper_paths.clone(), move || ctx.request_repaint());
        let ctx = cc.egui_ctx.clone();
        let downloads = crate::download::Downloader::new(piper_paths.clone(), move || ctx.request_repaint());

        let mut app = Self {
            sounds_dir_buf: sounds_dir.display().to_string(),
            paths,
            settings,
            settings_dirty: None,
            sounds_dir,
            entries: Vec::new(),
            meta,
            meta_dirty: None,
            hotkeys_dirty: true,
            player,
            ptt: AutoPtt::new(engine.clone()),
            engine,
            hotkeys,
            tts,
            piper: piper_paths,
            downloads,
            voice_url_buf: String::new(),
            devices: zv_audio::list_devices(),
            status: EngineStatus::default(),
            last_engine_config: None,
            tab: Tab::Sounds,
            search: String::new(),
            category: None,
            selected: None,
            rename_buf: String::new(),
            manual_hotkey_buf: String::new(),
            capture: None,
            pending_delete: None,
            toasts: Vec::new(),
            levels: Levels::default(),
            playing: HashSet::new(),
            tts_text: String::new(),
            tts_voices: HashMap::new(),
            tts_requested: HashSet::new(),
            tts_counter: 0,
            log_only_problems: false,
            inline_rename: None,
            editor: None,
            repaint_ctx: cc.egui_ctx.clone(),
            numpad,
            queue,
            ui_rx,
            played_rx,
            settings_page: SettingsPage::Devices,
            selected_tags: BTreeSet::new(),
            tag_input: String::new(),
            tag_rename: None,
            overlay_peek_until: None,
            overlay_preset_name: String::new(),
            timer_record_until: None,
            timer_form: timers_tab::TimerForm::default(),
            focus_search: false,
            waveform_cache: None,
            code_edit: None,
            code_conflict: None,
            pattern_ed: pattern_tab::PatternEditor::default(),
        };
        app.settings.migrate();
        // 0.9: в основной схеме появилась клавиша «сохранить что сейчас было».
        {
            let c = &mut app.settings.controls;
            let key = "Ctrl+Alt+B";
            if c.preset == zv_core::APP_TITLE
                && c.key(Action::SaveReplay).is_empty()
                && c.owner(key).is_none()
                && c.tag_owner(key).is_none()
                && app.meta.hotkey_owner(key).is_none()
            {
                c.bindings.insert(Action::SaveReplay, key.to_string());
            }
        }
        app.engine.set_effects(&app.settings.effects);
        app.init_timers();
        if app.tts.settings_plugin_missing(&app.settings.tts.plugin_id) {
            app.settings.tts.plugin_id = app.tts.plugins.first().map(|p| p.id.clone()).unwrap_or_default();
        }
        app.rescan();
        app.apply_audio_settings();
        if app.settings.replay.enabled {
            app.apply_replay();
        }
        if let Some(e) = app.hotkeys.init_error.clone() {
            app.toast_error(e);
        }
        app
    }

    // ---------- Сообщения ----------

    pub fn toast(&mut self, text: impl Into<String>) {
        let text = self.mask(&text.into());
        self.toasts.push(Toast { text, error: false, at: Instant::now() });
    }

    // ---------- Скрытие путей ----------

    /// Скрываются ли пути к папкам.
    pub fn paths_hidden(&self) -> bool {
        self.settings.ui.hide_paths
    }

    pub fn set_paths_hidden(&mut self, hide: bool) {
        self.settings.ui.hide_paths = hide;
        self.mark_settings_dirty();
    }

    /// Маскирует пути и имя пользователя в тексте, если включено скрытие.
    pub fn mask(&self, text: &str) -> String {
        if !self.paths_hidden() {
            return text.to_string();
        }
        let p = &self.paths;
        crate::privacy::Masker::new(
            &self.sounds_dir,
            &[(p.config_dir.as_path(), "%APPDATA%\\Zvukopad"), (p.local_dir.as_path(), "%LOCALAPPDATA%\\Zvukopad")],
        )
        .mask(text)
    }

    pub fn toast_error(&mut self, text: impl Into<String>) {
        let text = text.into();
        log::warn!("{text}");
        let text = self.mask(&text);
        self.toasts.push(Toast { text, error: true, at: Instant::now() });
    }

    // ---------- Сохранение ----------

    pub fn mark_settings_dirty(&mut self) {
        self.settings_dirty = Some(Instant::now());
        self.hotkeys_dirty = true;
    }

    pub fn mark_meta_dirty(&mut self) {
        self.meta_dirty = Some(Instant::now());
        self.hotkeys_dirty = true;
    }

    fn save_now(&mut self) {
        if self.settings_dirty.take().is_some()
            && let Err(e) = zv_core::save_json_atomic(&self.paths.settings_file(), &self.settings)
        {
            log::error!("Не удалось сохранить настройки: {e}");
        }
        if self.meta_dirty.take().is_some()
            && let Err(e) = zv_core::save_json_atomic(&self.paths.library_file(), &self.meta)
        {
            log::error!("Не удалось сохранить библиотеку: {e}");
        }
    }

    fn save_if_due(&mut self) {
        let due = |t: Option<Instant>| t.is_some_and(|t| t.elapsed() > Duration::from_millis(800));
        if due(self.settings_dirty) || due(self.meta_dirty) {
            self.save_now();
        }
    }

    // ---------- Библиотека ----------

    /// Перечитывает папку звуков.
    pub fn rescan(&mut self) {
        self.waveform_cache = None;
        self.entries = zv_core::library::scan(&self.sounds_dir);
        let slots: HashMap<String, SoundSlot> = self
            .entries
            .iter()
            .map(|e| {
                let m = self.meta.get(&e.id);
                (e.id.clone(), SoundSlot { path: e.path.clone(), volume: m.volume, has_hotkey: m.hotkey.is_some() })
            })
            .collect();
        self.player.set_slots(slots);
        if self.settings.numpad.auto_assign {
            let ids: Vec<String> = self.entries.iter().map(|e| e.id.clone()).collect();
            if self.meta.assign_codes(ids.iter().map(String::as_str)) {
                self.mark_meta_dirty();
            }
        }
        self.update_numpad_codes();
        if let Some(sel) = &self.selected
            && !self.entries.iter().any(|e| &e.id == sel)
        {
            self.selected = None;
        }
        self.hotkeys_dirty = true;
        log::info!("Звуков в библиотеке: {}", self.entries.len());
    }

    /// Обновляет таблицу «номер → звук» для набора на нумпаде.
    pub fn update_numpad_codes(&mut self) {
        let codes: HashMap<u32, String> = self
            .entries
            .iter()
            .filter_map(|e| self.meta.sounds.get(&e.id).and_then(|m| m.code).map(|c| (c, e.id.clone())))
            .collect();
        self.numpad.set_codes(codes);
        self.numpad.set_modifier(self.settings.numpad.modifier);
    }

    /// Задаёт звуку номер. Если номер был у другого звука — тот остаётся без номера.
    pub fn set_sound_code(&mut self, id: &str, code: Option<u32>) {
        if let Some(c) = code
            && let Some(owner) = self.meta.code_owner(c).map(str::to_string)
            && owner != id
        {
            self.meta.update(&owner, |m| m.code = None);
            let name = self.entry(&owner).map(|e| e.name.clone()).unwrap_or(owner);
            self.toast(tf!("Номер {} больше не у «{}»", c, name));
        }
        self.meta.update(id, |m| m.code = code);
        self.mark_meta_dirty();
        self.update_numpad_codes();
    }

    /// Звуки с тегом.
    pub fn ids_with_tag(&self, tag: &str) -> Vec<String> {
        self.entries
            .iter()
            .filter(|e| self.meta.sounds.get(&e.id).is_some_and(|m| m.tags.iter().any(|t| t == tag)))
            .map(|e| e.id.clone())
            .collect()
    }

    pub fn play_random_from_tag(&self, tag: &str) {
        self.player.play_random_from(&self.ids_with_tag(tag), &tf!("В теге «{}» нет звуков", tag));
    }

    /// Пики волны звука (считаются один раз на звук и ширину).
    pub fn waveform_peaks(&mut self, id: &str, data: &zv_audio::SoundData, buckets: usize) -> Vec<(f32, f32)> {
        if let Some((cid, n, p)) = &self.waveform_cache
            && cid == id
            && *n == buckets
        {
            return p.clone();
        }
        let peaks = data.peaks(buckets);
        self.waveform_cache = Some((id.to_string(), buckets, peaks.clone()));
        peaks
    }

    /// Применяет введённый номер. Если он занят другим звуком — спрашивает.
    pub fn commit_code_edit(&mut self) {
        let Some((id, buf)) = self.code_edit.take() else { return };
        let code = buf.trim().parse::<u32>().ok().filter(|c| *c > 0);
        if self.meta.get(&id).code == code {
            return;
        }
        if let Some(c) = code
            && let Some(owner) = self.meta.code_owner(c).map(str::to_string)
            && owner != id
        {
            self.code_conflict = Some((id, c, owner));
            return;
        }
        self.set_sound_code(&id, code);
    }

    /// Окно «номер занят — перезаписать?».
    fn code_conflict_dialog(&mut self, ctx: &egui::Context) {
        let Some((id, code, owner)) = self.code_conflict.clone() else { return };
        let owner_name = self.entry(&owner).map(|e| e.name.clone()).unwrap_or_else(|| owner.clone());
        let name = self.entry(&id).map(|e| e.name.clone()).unwrap_or_default();
        let mut decision: Option<bool> = None;
        egui::Modal::new(egui::Id::unique("code_conflict")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading(tf!("Номер {} уже занят", code));
            ui.add_space(6.0);
            ui.label(tf!("Сейчас этот номер у звука «{}».", owner_name));
            ui.label(tf!("Отдать его звуку «{}»? У «{}» номер пропадёт.", name, owner_name));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new(tr("Перезаписать")).color(style::ACCENT)).clicked() {
                    decision = Some(true);
                }
                if ui.button(tr("Отмена")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    decision = Some(false);
                }
            });
        });
        if let Some(yes) = decision {
            self.code_conflict = None;
            if yes {
                self.set_sound_code(&id, Some(code));
            }
        }
    }

    pub fn entry(&self, id: &str) -> Option<&SoundEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn categories(&self) -> Vec<String> {
        let mut c: Vec<String> = self.entries.iter().map(|e| e.category.clone()).collect();
        c.sort_by_key(|s| s.to_lowercase());
        c.dedup();
        c
    }

    pub fn select(&mut self, id: Option<String>) {
        if let Some(id) = &id
            && let Some(e) = self.entry(id)
        {
            self.rename_buf = e.name.clone();
            self.manual_hotkey_buf = self.meta.get(id).hotkey.unwrap_or_default();
        }
        self.selected = id;
    }

    pub fn start_inline_rename(&mut self, id: &str) {
        if let Some(e) = self.entry(id) {
            self.inline_rename = Some((id.to_string(), e.name.clone(), true));
        }
    }

    pub fn open_editor(&mut self, id: &str) {
        if let Some(e) = self.entry(id).cloned() {
            self.editor = Some(editor::EditorState::open(&e.id, &e.name, e.path.clone(), self.repaint_ctx.clone()));
        }
    }

    pub fn is_playing(&self, id: &str) -> bool {
        self.playing.contains(&sound_key(id))
    }

    /// Назначает (или снимает) горячую клавишу звуку. Если клавиша была у другого — переносит.
    pub fn set_sound_hotkey(&mut self, id: &str, hotkey: Option<String>) {
        if let Some(hk) = &hotkey {
            if let Some(owner) = self.meta.hotkey_owner(hk).map(str::to_string)
                && owner != id
            {
                let mut m = self.meta.get(&owner);
                m.hotkey = None;
                self.meta.set(&owner, m.clone());
                self.player.update_slot(&owner, m.volume, false);
                let name = self.entry(&owner).map(|e| e.name.clone()).unwrap_or(owner);
                self.toast(tf!("{} больше не у «{}»", hk, name));
            }
            if let Some(g) = self.global_hotkey_owner(hk) {
                self.toast_error(tf!("{} уже занята действием «{}»", hk, g));
                return;
            }
        }
        let mut m = self.meta.get(id);
        m.hotkey = hotkey;
        self.meta.set(id, m.clone());
        self.player.update_slot(id, m.volume, m.hotkey.is_some());
        self.manual_hotkey_buf = m.hotkey.unwrap_or_default();
        self.mark_meta_dirty();
    }

    pub fn global_hotkey_owner(&self, hk: &str) -> Option<&'static str> {
        if let Some(a) = self.settings.controls.owner(hk) {
            return Some(a.label());
        }
        if self.settings.controls.tag_owner(hk).is_some() {
            return Some(tr("случайный звук из тега"));
        }
        let n = &self.settings.numpad;
        if n.enabled {
            let lower = hk.to_lowercase();
            let prefix = n.modifier.prefix().to_lowercase();
            if lower.strip_prefix(&prefix).is_some_and(|k| k.starts_with("numpad")) {
                return Some(tr("набор номера на нумпаде"));
            }
        }
        None
    }

    fn rebuild_hotkeys(&mut self) {
        let mut bindings: Vec<(String, Bind)> = Vec::new();
        let n = &self.settings.numpad;
        if n.enabled {
            let p = n.modifier.prefix();
            for d in 0..=9u8 {
                bindings.push((format!("{p}Numpad{d}"), Bind::NumDigit(d)));
            }
            bindings.push((format!("{p}NumpadDecimal"), Bind::NumCancel));
        }
        let c = &self.settings.controls;
        for (a, k) in &c.bindings {
            if c.sound_hotkeys_enabled || *a == Action::ToggleSoundHotkeys || *a == Action::StopAll {
                bindings.push((k.clone(), Bind::Action(*a)));
            }
        }
        if c.sound_hotkeys_enabled {
            for (tag, k) in &c.tag_random {
                bindings.push((k.clone(), Bind::RandomSet(tag.clone(), Arc::new(self.ids_with_tag(tag)))));
            }
            for e in &self.entries {
                if let Some(hk) = self.meta.get(&e.id).hotkey {
                    bindings.push((hk, Bind::Sound(e.id.clone())));
                }
            }
        }
        let pass = c.pass_through;
        self.hotkeys.apply(bindings, pass);
    }

    /// Выполняет действие из горячей клавиши, которому нужно окно.
    pub fn run_action(&mut self, a: Action) {
        let step = self.settings.controls.volume_step;
        match a {
            Action::ModeCycle | Action::ModeBoth | Action::ModeMonitor | Action::ModeVirtual => {
                let t = match a {
                    Action::ModeBoth => PlayTarget::Both,
                    Action::ModeMonitor => PlayTarget::Monitor,
                    Action::ModeVirtual => PlayTarget::Virtual,
                    _ => self.settings.audio.target.next(),
                };
                self.settings.audio.target = t;
                self.player.set_config(play_config(&self.settings));
                self.mark_settings_dirty();
                self.toast(tf!("Играть: {}", t.label()));
            }
            Action::MonitorVolumeUp | Action::MonitorVolumeDown => {
                let d = if a == Action::MonitorVolumeUp { step } else { -step };
                let v = &mut self.settings.audio.monitor_volume;
                *v = (*v + d).clamp(0.0, 1.5);
                self.settings.audio.monitor_muted = false;
                let pct = (*v * 100.0).round();
                self.apply_audio_settings();
                self.mark_settings_dirty();
                self.toast(tf!("Громкость «себе»: {} %", pct));
            }
            Action::VirtualVolumeUp | Action::VirtualVolumeDown => {
                let d = if a == Action::VirtualVolumeUp { step } else { -step };
                let v = &mut self.settings.audio.virtual_volume;
                *v = (*v + d).clamp(0.0, 1.5);
                let pct = (*v * 100.0).round();
                self.apply_audio_settings();
                self.mark_settings_dirty();
                self.toast(tf!("Громкость «в микрофон»: {} %", pct));
            }
            Action::MonitorMute => {
                self.settings.audio.monitor_muted = !self.settings.audio.monitor_muted;
                self.apply_audio_settings();
                self.mark_settings_dirty();
                self.toast(if self.settings.audio.monitor_muted {
                    tr("«Себе»: без звука")
                } else {
                    tr("«Себе»: звук включён")
                });
            }
            Action::ToggleOverlay => {
                let shown = self.overlay_visible();
                self.settings.overlay.mode = OverlayMode::Toggle;
                self.settings.overlay.visible = !shown;
                self.overlay_peek_until = None;
                self.mark_settings_dirty();
            }
            Action::PeekOverlay => {
                self.overlay_peek_until =
                    Some(Instant::now() + Duration::from_secs_f32(self.settings.overlay.peek_secs.max(0.5)));
            }
            Action::ToggleRecording => self.toggle_recording(),
            Action::ToggleEffects => {
                self.settings.effects.enabled = !self.settings.effects.enabled;
                self.engine.set_effects(&self.settings.effects);
                self.mark_settings_dirty();
                self.toast(if self.settings.effects.enabled {
                    tr("Эффекты включены")
                } else {
                    tr("Эффекты выключены")
                });
            }
            Action::ToggleSoundHotkeys => {
                let c = &mut self.settings.controls;
                c.sound_hotkeys_enabled = !c.sound_hotkeys_enabled;
                let on = c.sound_hotkeys_enabled;
                self.mark_settings_dirty();
                self.toast(if on {
                    tr("Клавиши звуков включены")
                } else {
                    tr("Клавиши звуков выключены")
                });
            }
            Action::SaveReplay => self.save_replay(None),
            Action::ToggleReplay => {
                let on = !self.settings.replay.enabled;
                self.set_replay(on);
                self.toast(if on {
                    tr("Буфер повтора включён")
                } else {
                    tr("Буфер повтора выключен")
                });
            }
            // Эти действия выполняются сразу в обработчике клавиш.
            Action::StopAll
            | Action::PauseResume
            | Action::RepeatLast
            | Action::PlayRandom
            | Action::QueuePlayStop
            | Action::QueueNext => {}
        }
    }

    /// Виден ли сейчас оверлей.
    pub fn overlay_visible(&self) -> bool {
        let o = &self.settings.overlay;
        if o.show_on_numpad && self.settings.numpad.enabled && !self.numpad.buffer().is_empty() {
            return true;
        }
        if self.overlay_peek_until.is_some_and(|t| Instant::now() < t) {
            return true;
        }
        match o.mode {
            OverlayMode::Off | OverlayMode::Peek => false,
            OverlayMode::Always => true,
            OverlayMode::Toggle => o.visible,
        }
    }

    // ---------- Таймеры ----------

    fn init_timers(&mut self) {
        let now = now_unix();
        let local = sysutil::local_secs_of_day();
        let mut changed = false;
        for t in &mut self.settings.timers {
            if !t.enabled {
                continue;
            }
            match t.next_due {
                Some(due) if due + 5 < now => {
                    log::info!("Таймер {} пропущен, пока программа была закрыта", t.id);
                    t.after_fire(now, local);
                    changed = true;
                }
                None => {
                    t.schedule(now, local);
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            self.mark_settings_dirty();
        }
    }

    fn tick_timers(&mut self) {
        let now = now_unix();
        if let Some(until) = self.timer_record_until {
            if !self.engine.is_recording() {
                self.timer_record_until = None;
            } else if now >= until {
                self.timer_record_until = None;
                self.engine.stop_recording();
                log::info!("Таймер: запись остановлена");
            }
        }
        let local = sysutil::local_secs_of_day();
        let mut fired: Vec<TimerKind> = Vec::new();
        for t in &mut self.settings.timers {
            if t.enabled && t.next_due.is_some_and(|d| now >= d) {
                fired.push(t.kind.clone());
                t.after_fire(now, local);
            }
        }
        if fired.is_empty() {
            return;
        }
        self.mark_settings_dirty();
        for kind in fired {
            match kind {
                TimerKind::Record { secs } => {
                    if !self.engine.is_recording() {
                        self.toggle_recording();
                    }
                    self.timer_record_until = Some(now + secs.max(1) as u64);
                    self.toast(tf!("Таймер: запись на {}", widgets::fmt_time(secs as f64)));
                }
                TimerKind::PlaySound { id } => {
                    let name = self.entry(&id).map(|e| e.name.clone());
                    match name {
                        Some(n) => {
                            self.player.play(&id, None);
                            log::info!("Таймер: играю «{n}»");
                        }
                        None => self.toast_error(tr("Таймер: звук не найден (удалён?)")),
                    }
                }
                TimerKind::PlayQueue => {
                    self.queue.play();
                    log::info!("Таймер: запущена очередь");
                }
            }
        }
    }

    // ---------- Звук ----------

    pub fn apply_audio_settings(&mut self) {
        let a = &self.settings.audio;
        self.engine.set_monitor_volume(if a.monitor_muted { 0.0 } else { a.monitor_volume });
        self.engine.set_virtual_volume(a.virtual_volume);
        self.engine.set_mic_volume(a.mic_volume);
        self.engine.set_ducking(a.duck_enabled, a.duck_threshold, a.duck_level);
        let p = &self.settings.auto_ptt;
        self.ptt.configure(p.enabled, &p.key, p.release_ms);
        self.player.set_config(play_config(&self.settings));
        let cfg = engine_config(&self.settings);
        if self.last_engine_config.as_ref() != Some(&cfg) {
            self.engine.configure(cfg.clone());
            self.last_engine_config = Some(cfg);
        }
    }

    pub fn refresh_devices(&mut self) {
        self.devices = zv_audio::list_devices();
    }

    pub fn toggle_recording(&mut self) {
        if self.engine.is_recording() {
            self.engine.stop_recording();
            return;
        }
        let dir = self.sounds_dir.join(&self.settings.recording.folder);
        let stem = tf!("Запись {}", sysutil::local_timestamp());
        let path = zv_core::library::unique_path(&dir, &stem, "wav");
        let r = &self.settings.recording;
        self.engine.start_recording(r.source.clone(), r.loopback, path);
    }

    // ---------- Язык ----------

    /// Меняет язык интерфейса сразу, без перезапуска.
    pub fn set_language(&mut self, setting: String, ctx: &egui::Context) {
        crate::sysutil::apply_language(&setting);
        self.settings.ui.language = setting;
        self.mark_settings_dirty();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(zv_core::app_title().to_string()));
        self.reload_voices(&self.settings.tts.plugin_id.clone());
    }

    // ---------- Буфер «что сейчас было» ----------

    /// Включает/выключает буфер и запоминает выбор.
    pub fn set_replay(&mut self, on: bool) {
        self.settings.replay.enabled = on;
        self.apply_replay();
        self.mark_settings_dirty();
    }

    /// Применяет настройки буфера к движку (перезапускает его с новым источником/длиной).
    pub fn apply_replay(&mut self) {
        let r = &self.settings.replay;
        if r.enabled {
            self.engine.start_replay(r.source.clone(), r.loopback, r.secs);
        } else {
            self.engine.stop_replay();
        }
    }

    /// Сохраняет последние `secs` секунд буфера (`None` — всё, что есть).
    pub fn save_replay(&mut self, secs: Option<f64>) {
        if self.engine.replay_info().is_none() {
            self.toast_error(tr("Буфер повтора выключен — включите его на вкладке «Запись»"));
            return;
        }
        let dir = self.sounds_dir.join(&self.settings.replay.folder);
        let stem = tf!("Повтор {}", sysutil::local_timestamp());
        let path = zv_core::library::unique_path(&dir, &stem, "wav");
        self.engine.save_replay(secs.unwrap_or(f64::MAX), path);
    }

    // ---------- Синтез речи ----------

    pub fn ensure_voices(&mut self, plugin_id: &str) {
        if !plugin_id.is_empty() && self.tts_requested.insert(plugin_id.to_string()) {
            self.tts.request_voices(plugin_id);
        }
    }

    pub fn reload_voices(&mut self, plugin_id: &str) {
        self.tts_requested.remove(plugin_id);
        self.tts_voices.remove(plugin_id);
        self.ensure_voices(plugin_id);
    }

    pub fn synthesize(&mut self, purpose: SynthPurpose) {
        let text = self.tts_text.trim().to_string();
        if text.is_empty() {
            self.toast_error(tr("Введите текст"));
            return;
        }
        let t = &self.settings.tts;
        let output = match purpose {
            SynthPurpose::Save => {
                let dir = self.sounds_dir.join(&t.folder);
                let stem: String = zv_core::library::sanitize_file_name(&text.chars().take(40).collect::<String>());
                zv_core::library::unique_path(&dir, &stem, "wav")
            }
            _ => {
                self.tts_counter += 1;
                self.paths.temp_dir().join(format!("tts-{}-{}.wav", std::process::id(), self.tts_counter))
            }
        };
        let request = zv_plugins::SynthRequest { text, voice: t.voice_id.clone(), rate: t.rate, output };
        let plugin = t.plugin_id.clone();
        self.tts.synthesize(&plugin, request, purpose);
    }

    fn handle_tts_events(&mut self) {
        for ev in self.tts.take_events() {
            match ev {
                TtsEvent::Voices { plugin_id, result } => {
                    if let Ok(voices) = &result
                        && plugin_id == self.settings.tts.plugin_id
                        && !voices.iter().any(|v| v.id == self.settings.tts.voice_id)
                    {
                        // Предпочитаем русский голос, если он есть.
                        let pick = voices.iter().find(|v| v.language.starts_with("ru")).or(voices.first());
                        if let Some(v) = pick {
                            self.settings.tts.voice_id = v.id.clone();
                            self.mark_settings_dirty();
                        }
                    }
                    self.tts_voices.insert(plugin_id, result);
                }
                TtsEvent::Synthesized { purpose, result } => match result {
                    Err(e) => self.toast_error(tf!("Синтез речи: {}", e)),
                    Ok(path) => match purpose {
                        SynthPurpose::Save => {
                            self.rescan();
                            let id = zv_core::library::id_for(&self.sounds_dir, &path);
                            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                            self.toast(tf!("Сохранено в библиотеку: «{}»", name));
                            self.select(id);
                        }
                        SynthPurpose::Preview | SynthPurpose::Say => {
                            let decoded = zv_audio::decode_file(&path);
                            let _ = std::fs::remove_file(&path);
                            match decoded {
                                Ok(data) => {
                                    let target = if purpose == SynthPurpose::Preview {
                                        PlayTarget::Monitor
                                    } else {
                                        self.settings.audio.target
                                    };
                                    self.player.play_adhoc("__tts__", Arc::new(data), target);
                                }
                                Err(e) => self.toast_error(tf!("Синтез речи: {}", e)),
                            }
                        }
                    },
                },
            }
        }
    }

    // ---------- Фоновая логика ----------

    /// Выполняется каждый кадр, даже когда окно свёрнуто.
    fn tick(&mut self, ctx: &egui::Context) {
        while let Ok(UiEvent::Action(a)) = self.ui_rx.try_recv() {
            self.run_action(a);
        }
        // Показ оверлея при запуске звука, пока окно программы не в фокусе (то есть из игры).
        let mut played_any = false;
        while self.played_rx.try_recv().is_ok() {
            played_any = true;
        }
        if played_any && self.settings.overlay.show_on_play && !ctx.input(|i| i.focused) {
            self.overlay_peek_until =
                Some(Instant::now() + Duration::from_secs_f32(self.settings.overlay.peek_secs.max(0.5)));
        }
        self.tick_timers();
        for msg in self.player.take_messages() {
            self.toast_error(msg);
        }
        for res in self.engine.take_recording_results() {
            match res {
                RecordingResult::Saved(path) => {
                    self.rescan();
                    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                    let replay_dir = self.sounds_dir.join(&self.settings.replay.folder);
                    if path.parent() == Some(replay_dir.as_path()) {
                        self.toast(tf!("Сохранено «что сейчас было»: «{}»", name));
                    } else {
                        self.toast(tf!("Запись сохранена: «{}»", name));
                        let id = zv_core::library::id_for(&self.sounds_dir, &path);
                        self.select(id);
                    }
                }
                RecordingResult::Failed(e) => {
                    self.rescan();
                    self.toast_error(tf!("Запись: {}", e));
                }
            }
        }
        for res in self.downloads.take_results() {
            match res {
                Ok(m) => {
                    self.toast(tf!("Готово: {}", m));
                    self.reload_voices(crate::piper::PIPER_ID);
                }
                Err(e) => self.toast_error(tf!("Загрузка: {}", e)),
            }
        }
        if let Some(e) = self.engine.take_replay_error() {
            self.toast_error(tf!("Буфер повтора: {}", e));
            self.settings.replay.enabled = false;
            self.mark_settings_dirty();
        }
        self.handle_tts_events();

        if self.hotkeys_dirty {
            self.hotkeys_dirty = false;
            self.rebuild_hotkeys();
        }
        self.hotkeys.set_paused(self.capture.is_some());

        self.status = self.engine.status();
        self.playing = self.engine.playing_keys();
        let l = self.engine.take_levels();
        let decay = |old: f32, new: f32| new.max(old * 0.85);
        self.levels = Levels {
            monitor: decay(self.levels.monitor, l.monitor),
            virtual_out: decay(self.levels.virtual_out, l.virtual_out),
            mic: decay(self.levels.mic, l.mic),
            recording: decay(self.levels.recording, l.recording),
        };
        self.toasts.retain(|t| t.at.elapsed() < Duration::from_secs(if t.error { 8 } else { 5 }));
        self.save_if_due();

        // Пока что-то звучит или пишется, перерисовываемся для индикаторов и оверлея.
        let busy = !self.playing.is_empty()
            || self.status.recording.is_some()
            || self.levels.mic > 0.01
            || self.tts.busy
            || self.settings_dirty.is_some()
            || self.meta_dirty.is_some();
        let queue_on = self.queue.snapshot().playing;
        if busy || queue_on || !self.numpad.buffer().is_empty() {
            ctx.request_repaint_after(Duration::from_millis(50));
        } else if let Some(t) = self.overlay_peek_until {
            if Instant::now() < t {
                ctx.request_repaint_after(Duration::from_millis(100));
            } else {
                self.overlay_peek_until = None;
                ctx.request_repaint();
            }
        } else if !self.toasts.is_empty() || self.overlay_visible() {
            ctx.request_repaint_after(Duration::from_millis(250));
        } else if self.settings.timers.iter().any(|t| t.enabled) || self.timer_record_until.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    // ---------- Отрисовка ----------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(zv_core::app_title()).size(20.0).strong().color(style::ACCENT));
            ui.add_space(12.0);
            ui.label(tr("Играть:"));
            let mut target = self.settings.audio.target;
            for t in [PlayTarget::Monitor, PlayTarget::Virtual, PlayTarget::Both] {
                ui.selectable_value(&mut target, t, t.label());
            }
            if target != self.settings.audio.target {
                self.settings.audio.target = target;
                self.player.set_config(play_config(&self.settings));
                self.mark_settings_dirty();
            }
            ui.add_space(8.0);
            let stop = egui::Button::new(RichText::new(tr("■ Стоп")).strong());
            if ui
                .add(stop)
                .on_hover_text(tf!("Остановить все звуки ({})", self.settings.controls.key(Action::StopAll)))
                .clicked()
            {
                self.queue.stop();
                self.player.stop_all();
            }
            let paused = self.engine.is_paused();
            if ui
                .selectable_label(paused, if paused { tr("▶ Продолжить") } else { tr("⏸ Пауза") })
                .on_hover_text(tf!("Пауза / продолжить ({})", self.settings.controls.key(Action::PauseResume)))
                .clicked()
            {
                self.player.toggle_pause();
            }
            if ui
                .button("↻")
                .on_hover_text(tf!("Повторить последний ({})", self.settings.controls.key(Action::RepeatLast)))
                .clicked()
            {
                self.player.repeat_last();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(lang) = widgets::language_button(ui, &self.settings.ui.language.clone()) {
                    self.set_language(lang, ui.ctx());
                }
                let mut ov = self.overlay_visible();
                if ui
                    .toggle_value(&mut ov, tr("Оверлей"))
                    .on_hover_text(tf!(
                        "Окно-подсказка поверх игр ({})",
                        self.settings.controls.key(Action::ToggleOverlay)
                    ))
                    .changed()
                {
                    self.settings.overlay.mode = OverlayMode::Toggle;
                    self.settings.overlay.visible = ov;
                    self.overlay_peek_until = None;
                    self.mark_settings_dirty();
                }
                let mut fx = self.settings.effects.enabled;
                if ui
                    .toggle_value(&mut fx, tr("🎛 Эффекты"))
                    .on_hover_text(tf!("Эффекты вкл/выкл ({})", self.settings.controls.key(Action::ToggleEffects)))
                    .changed()
                {
                    self.settings.effects.enabled = fx;
                    self.engine.set_effects(&self.settings.effects);
                    self.mark_settings_dirty();
                }
                if !self.settings.controls.sound_hotkeys_enabled {
                    ui.label(RichText::new(tr("⌨ клавиши звуков выкл")).color(style::DANGER));
                }
                if self.settings.audio.monitor_muted {
                    ui.label(RichText::new(tr("🔇 «себе» без звука")).color(style::DANGER));
                }
                if self.ptt.is_pressed() {
                    ui.label(RichText::new(tr("📻 рация")).color(style::ACCENT))
                        .on_hover_text(tr("Клавиша рации нажата, пока звучит звук"));
                }
                if let Some(r) = self.engine.replay_info() {
                    let key = self.settings.controls.key(Action::SaveReplay).to_string();
                    let hint = if key.is_empty() {
                        tr("Буфер «что сейчас было» включён. Нажмите, чтобы сохранить").to_string()
                    } else {
                        tf!("Буфер «что сейчас было» включён. Нажмите или {}, чтобы сохранить", key)
                    };
                    if ui
                        .button(RichText::new(tf!("⏪ {} с", r.capacity_secs)).color(style::ACCENT))
                        .on_hover_text(hint)
                        .clicked()
                    {
                        self.save_replay(None);
                    }
                }
                if self.status.recording.is_some() {
                    let secs = self.engine.recorded_seconds();
                    let blink = (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0;
                    let dot = if blink { "●" } else { "○" };
                    let left = self.timer_record_until.map(|u| u.saturating_sub(now_unix()));
                    let text = match left {
                        Some(l) => {
                            tf!("{} Запись {} (ещё {})", dot, widgets::fmt_time(secs), widgets::fmt_time(l as f64))
                        }
                        None => tf!("{} Запись {}", dot, widgets::fmt_time(secs)),
                    };
                    if ui
                        .button(RichText::new(text).color(style::DANGER))
                        .on_hover_text(tr("Остановить запись"))
                        .clicked()
                    {
                        self.engine.stop_recording();
                    }
                }
            });
        });
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (tab, label) in [
                (Tab::Sounds, tr("🔊 Звуки")),
                (Tab::Record, tr("⏺ Запись")),
                (Tab::Tts, tr("🗣 Синтез речи")),
                (Tab::Effects, tr("🎛 Эффекты")),
                (Tab::Pattern, tr("🥁 Редактор")),
                (Tab::Timers, tr("⏰ Таймеры")),
                (Tab::Settings, tr("⚙ Настройки")),
            ] {
                ui.selectable_value(&mut self.tab, tab, RichText::new(label).size(15.0));
            }
            // Журнал: число ошибок и предупреждений видно прямо на вкладке.
            let problems = crate::logger::problem_count();
            let log_label = if problems > 0 {
                RichText::new(tf!("📜 Журнал ({})", problems)).size(15.0).color(style::DANGER)
            } else {
                RichText::new(tr("📜 Журнал")).size(15.0)
            };
            ui.selectable_value(&mut self.tab, Tab::Log, log_label);
            ui.selectable_value(&mut self.tab, Tab::Help, RichText::new(tr("❓ Справка")).size(15.0));
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let st = self.status.clone();
            let st = &st;
            let item = |ui: &mut egui::Ui, name: &str, s: &zv_audio::StreamStatus, off: &str| {
                if s.active {
                    ui.label(RichText::new(format!("{name}: {}", s.device)).color(style::MUTED));
                } else if let Some(e) = &s.error {
                    ui.label(RichText::new(format!("{name}: {e}")).color(style::DANGER));
                } else {
                    ui.label(RichText::new(format!("{name}: {off}")).color(style::MUTED));
                }
            };
            let levels = self.levels;
            let (mon, virt, mic) = (st.monitor.active, st.virtual_out.active, st.mic.active);
            let mut changed = false;
            let show_mic = self.settings.audio.virtual_device.is_some() && self.settings.audio.mic_passthrough;
            let audio = &mut self.settings.audio;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Наведите на индикатор (или нажмите) — появится ползунок громкости.
                let r = widgets::level_meter_resp(ui, tr("Мой голос"), levels.mic, mic, false);
                changed |= volume_popup(r, tr("Громкость моего голоса:"), &mut audio.mic_volume, 2.0, None);
                let r = widgets::level_meter_resp(ui, tr("В микрофон"), levels.virtual_out, virt, false);
                changed |= volume_popup(r, tr("Громкость «в микрофон»:"), &mut audio.virtual_volume, 1.5, None);
                let r = widgets::level_meter_resp(ui, tr("Себе"), levels.monitor, mon, false);
                changed |= volume_popup(
                    r,
                    tr("Громкость «себе»:"),
                    &mut audio.monitor_volume,
                    1.5,
                    Some(&mut audio.monitor_muted),
                );
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    item(ui, tr("Себе"), &st.monitor, "—");
                    ui.separator();
                    item(ui, tr("В микрофон"), &st.virtual_out, tr("не настроено"));
                    if show_mic {
                        ui.separator();
                        item(ui, tr("Микрофон"), &st.mic, "—");
                    }
                });
            });
            if changed {
                self.apply_audio_settings();
                self.mark_settings_dirty();
            }
        });
    }

    fn show_toasts(&mut self, ctx: &egui::Context) {
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -36.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                for t in self.toasts.iter().rev().take(4) {
                    let color = if t.error { style::DANGER } else { style::ACCENT };
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgb(30, 31, 38))
                        .stroke(egui::Stroke::new(1.0, color))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.set_max_width(420.0);
                            ui.label(RichText::new(&t.text).color(egui::Color32::WHITE));
                        });
                    ui.add_space(4.0);
                }
            });
    }

    /// Файлы, брошенные в окно, копируются в библиотеку.
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect()
        });
        if dropped.is_empty() {
            return;
        }
        let category = self.category.clone().unwrap_or_default();
        let (mut ok, mut skipped) = (0, Vec::new());
        let mut last = None;
        for path in dropped {
            match zv_core::library::import_file(&self.sounds_dir, &category, &path) {
                Ok(p) => {
                    ok += 1;
                    last = Some(p);
                }
                Err(e) => skipped.push(format!("{}: {e}", path.file_name().unwrap_or_default().to_string_lossy())),
            }
        }
        self.rescan();
        if ok > 0 {
            self.toast(tf!("Добавлено звуков: {}", ok));
            if let Some(p) = last {
                let id = zv_core::library::id_for(&self.sounds_dir, &p);
                self.select(id);
            }
            self.tab = Tab::Sounds;
        }
        for s in skipped {
            self.toast_error(tf!("Не добавлен {}", s));
        }
    }
}

impl eframe::App for ZvApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_dropped_files(&ctx);

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(2.0);
            self.tabs(ui);
            ui.add_space(2.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));

        match self.tab {
            Tab::Sounds => sounds::show(self, ui),
            Tab::Record => record::show(self, ui),
            Tab::Tts => tts_tab::show(self, ui),
            Tab::Effects => effects_tab::show(self, ui),
            Tab::Pattern => pattern_tab::show(self, ui),
            Tab::Timers => timers_tab::show(self, ui),
            Tab::Settings => settings_tab::show(self, ui),
            Tab::Log => log_tab::show(self, ui),
            Tab::Help => help::show(self, ui),
        }

        // Выбрали другой звук или закрыли карточку посреди ввода номера — это тоже «клик мимо».
        if self.code_edit.as_ref().is_some_and(|(id, _)| self.selected.as_deref() != Some(id.as_str())) {
            self.commit_code_edit();
        }
        self.code_conflict_dialog(&ctx);
        widgets::capture_dialog(self, &ctx);
        sounds::delete_dialog(self, &ctx);
        editor::show(self, &ctx);
        self.show_toasts(&ctx);

        // Подсказка при перетаскивании файлов.
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let screen = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop")));
            painter.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(170));
            painter.text(
                screen.center(),
                egui::Align2::CENTER_CENTER,
                tr("Отпустите, чтобы добавить звуки в библиотеку"),
                egui::FontId::proportional(24.0),
                egui::Color32::WHITE,
            );
        }

        if self.overlay_visible() {
            overlay::show(self, &ctx);
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Прозрачный фон нужен оверлею; основное окно всё равно закрашено панелями.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let dir = self.paths.patterns_dir();
        self.pattern_ed.save_if_due(&dir, true);
        if self.engine.is_recording() {
            self.engine.stop_recording();
        }
        self.queue.stop();
        self.settings_dirty = Some(Instant::now());
        self.meta_dirty = Some(Instant::now());
        self.save_now();
        self.engine.shutdown();
        // Даём движку дописать запись.
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub fn engine_config(s: &Settings) -> EngineConfig {
    EngineConfig {
        monitor: s.audio.monitor_device.clone(),
        virtual_out: s.audio.virtual_device.clone(),
        mic: s.audio.mic_device.clone(),
        mic_enabled: s.audio.mic_passthrough,
    }
}

pub fn play_config(s: &Settings) -> PlayConfig {
    PlayConfig { target: s.audio.target, mode: s.audio.play_mode, normalize: s.audio.normalize }
}

impl TtsService {
    fn settings_plugin_missing(&self, id: &str) -> bool {
        !self.plugins.iter().any(|p| p.id == id)
    }
}

/// Ползунок громкости во всплывающей подсказке у индикатора (наведение) или в меню (клик).
fn volume_popup(resp: egui::Response, title: &str, value: &mut f32, max: f32, mut muted: Option<&mut bool>) -> bool {
    let mut changed = false;
    let mut body = |ui: &mut egui::Ui| {
        ui.label(RichText::new(title).strong());
        ui.horizontal(|ui| {
            changed |= widgets::volume_slider(ui, value, max).changed();
            if let Some(m) = muted.as_deref_mut() {
                changed |= ui.checkbox(m, tr("без звука")).changed();
            }
        });
    };
    let resp = resp.on_hover_ui(|ui| body(ui));
    egui::Popup::menu(&resp).show(|ui| body(ui));
    changed
}
