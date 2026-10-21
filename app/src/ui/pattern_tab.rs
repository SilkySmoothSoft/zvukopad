//! Вкладка «Редактор»: дорожки со звуками на сетке, темп, высота и громкость каждого звука,
//! прослушивание по кругу и сохранение результата в WAV (он сразу попадает в библиотеку).
//!
//! Управление:
//! * перетащите звук из списка слева на дорожку — или выберите его кликом («кисть»)
//!   и кликайте по клеткам дорожки;
//! * звук на дорожке можно таскать мышью (прилипает к сетке), правый клик — меню;
//! * Del — удалить выбранный звук, Ctrl+D — копия следом, пробел — играть/стоп.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};
use zv_core::i18n::tr;
use zv_core::pattern::{Clip, GRID_STEPS, Pattern, Track};
use zv_core::tf;

use super::{ZvApp, style, widgets};
use crate::pattern_render;

/// Ключ, под которым паттерн играет в движке.
const KEY: &str = "__pattern__";
const LANE_H: f32 = 52.0;
const HEADER_W: f32 = 170.0;
const RULER_H: f32 = 22.0;

struct Drag {
    track: usize,
    clip: usize,
    /// Где схватили звук — сколько долей от его начала.
    grab: f64,
}

pub struct PatternEditor {
    pub pattern: Pattern,
    dirty: Option<Instant>,
    /// Имя файла, под которым паттерн сейчас сохранён (для переименования).
    saved_name: String,
    selected: Option<(usize, usize)>,
    brush: Option<String>,
    zoom: f32,
    search: String,
    drag: Option<Drag>,
    render_needed: bool,
    last_render: Option<Instant>,
    missing: Vec<String>,
    menu_clip: Option<(usize, usize)>,
    name_buf: String,
    loaded: bool,
}

impl Default for PatternEditor {
    fn default() -> Self {
        Self {
            pattern: Pattern::default(),
            dirty: None,
            saved_name: String::new(),
            selected: None,
            brush: None,
            zoom: 60.0,
            search: String::new(),
            drag: None,
            render_needed: false,
            last_render: None,
            missing: Vec::new(),
            menu_clip: None,
            name_buf: String::new(),
            loaded: false,
        }
    }
}

// ---------- Файлы паттернов ----------

fn file_for(dir: &std::path::Path, name: &str) -> PathBuf {
    dir.join(format!("{}.json", zv_core::library::sanitize_file_name(name)))
}

/// Сохранённые паттерны (имена), новые сверху.
fn list_patterns(dir: &std::path::Path) -> Vec<String> {
    let mut v: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let t = e.metadata().and_then(|m| m.modified()).ok()?;
            let name = e.path().file_stem()?.to_string_lossy().into_owned();
            Some((t, name))
        })
        .collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.0));
    v.into_iter().map(|(_, n)| n).collect()
}

fn new_name(dir: &std::path::Path) -> String {
    let existing = list_patterns(dir);
    (1..).map(|n| tf!("Паттерн {}", n)).find(|n| !existing.contains(n)).unwrap_or_default()
}

impl PatternEditor {
    fn mark(&mut self) {
        self.dirty = Some(Instant::now());
        self.render_needed = true;
    }

    fn load_initial(&mut self, dir: &std::path::Path) {
        self.loaded = true;
        if let Some(name) = list_patterns(dir).first() {
            self.open(dir, name);
        } else {
            self.pattern = Pattern { name: new_name(dir), ..Default::default() };
            self.name_buf = self.pattern.name.clone();
        }
    }

    fn open(&mut self, dir: &std::path::Path, name: &str) {
        let mut p: Pattern = zv_core::load_json_or_default(&file_for(dir, name));
        if p.name.is_empty() {
            p.name = name.to_string();
        }
        if p.tracks.is_empty() {
            p.tracks.push(Track::default());
        }
        self.saved_name = p.name.clone();
        self.name_buf = p.name.clone();
        self.pattern = p;
        self.selected = None;
        self.render_needed = true;
    }

    /// Сохраняет, если были изменения (не чаще раза в полсекунды).
    pub fn save_if_due(&mut self, dir: &std::path::Path, force: bool) {
        let due = self.dirty.is_some_and(|t| force || t.elapsed() > Duration::from_millis(500));
        if !due {
            return;
        }
        self.dirty = None;
        if self.pattern.name.trim().is_empty() {
            return;
        }
        if !self.saved_name.is_empty() && self.saved_name != self.pattern.name {
            let _ = std::fs::remove_file(file_for(dir, &self.saved_name));
        }
        if let Err(e) = zv_core::save_json_atomic(&file_for(dir, &self.pattern.name), &self.pattern) {
            log::error!("Паттерн не сохранён: {e}");
        }
        self.saved_name = self.pattern.name.clone();
    }

    fn clip_mut(&mut self, (t, c): (usize, usize)) -> Option<&mut Clip> {
        self.pattern.tracks.get_mut(t)?.clips.get_mut(c)
    }
}

// ---------- Звуки для сведения ----------

/// Загруженные звуки паттерна. `None` — что-то ещё грузится (попробуем в следующем кадре).
fn gather_sounds(app: &ZvApp) -> Option<HashMap<String, Arc<zv_audio::SoundData>>> {
    let mut map = HashMap::new();
    let mut waiting = false;
    for id in app.pattern_ed.pattern.sounds() {
        if let Some(d) = app.player.cached_data(&id) {
            map.insert(id, d);
        } else if app.entry(&id).is_some() {
            app.player.preload(&id);
            waiting = true;
        }
    }
    (!waiting).then_some(map)
}

fn is_playing(app: &ZvApp) -> bool {
    app.player.position(KEY).is_some()
}

/// Сводит и запускает. `keep_pos` — продолжить с того же места (после правки во время игры).
fn play(app: &mut ZvApp, keep_pos: bool) -> bool {
    let Some(sounds) = gather_sounds(app) else { return false };
    let pos = if keep_pos { app.player.position(KEY).map(|(s, _)| s) } else { None };
    let (data, missing) = pattern_render::render(&app.pattern_ed.pattern, &sounds);
    app.pattern_ed.missing = missing;
    let len = data.duration_secs();
    let looping = app.pattern_ed.pattern.looping;
    app.player.play_pattern(KEY, Arc::new(data), app.settings.audio.target, looping);
    if let Some(p) = pos
        && len > 0.0
    {
        app.player.seek(KEY, p % len);
    }
    app.pattern_ed.render_needed = false;
    app.pattern_ed.last_render = Some(Instant::now());
    true
}

fn stop(app: &mut ZvApp) {
    app.player.stop(KEY);
}

/// Сохраняет сведённый паттерн как звук в библиотеку.
fn export(app: &mut ZvApp) {
    let Some(sounds) = gather_sounds(app) else {
        app.toast(tr("Звуки ещё загружаются — попробуйте через секунду"));
        return;
    };
    let p = &app.pattern_ed.pattern;
    if p.tracks.iter().all(|t| t.clips.is_empty()) {
        app.toast_error(tr("Паттерн пустой — добавьте звуки на дорожки"));
        return;
    }
    let (data, _) = pattern_render::render(p, &sounds);
    let dir = app.sounds_dir.join(tr("Паттерны"));
    let stem = zv_core::library::sanitize_file_name(&p.name);
    let path = zv_core::library::unique_path(&dir, &stem, "wav");
    let _ = std::fs::create_dir_all(&dir);
    match data.write_wav(&path) {
        Ok(()) => {
            app.rescan();
            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            app.toast(tf!("Сохранено в библиотеку: «{}»", name));
        }
        Err(e) => app.toast_error(tf!("Не удалось сохранить: {}", e)),
    }
}

// ---------- Интерфейс ----------

pub fn show(app: &mut ZvApp, ui: &mut egui::Ui) {
    let dir = app.paths.patterns_dir();
    if !app.pattern_ed.loaded {
        app.pattern_ed.load_initial(&dir);
    }
    egui::Panel::left("pattern_palette").resizable(true).default_size(210.0).show(ui, |ui| palette(app, ui));
    egui::CentralPanel::default().show(ui, |ui| {
        toolbar(app, ui, &dir);
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("pattern_v")
            .auto_shrink([false, true])
            .max_height(ui.available_height() - 120.0)
            .show(ui, |ui| timeline(app, ui));
        ui.separator();
        inspector(app, ui);
    });
    keyboard(app, ui);

    // Правка во время игры — пересводим (не чаще ~8 раз в секунду и не посреди перетаскивания).
    let ed = &app.pattern_ed;
    if ed.render_needed
        && ed.drag.is_none()
        && is_playing(app)
        && ed.last_render.is_none_or(|t| t.elapsed() > Duration::from_millis(120))
    {
        play(app, true);
    }
    if is_playing(app) {
        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
    app.pattern_ed.save_if_due(&dir, false);
}

fn palette(app: &mut ZvApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.label(RichText::new(tr("Звуки")).strong());
    ui.add(egui::TextEdit::singleline(&mut app.pattern_ed.search).hint_text(tr("поиск")));
    ui.label(
        RichText::new(tr("Перетащите звук на дорожку или выберите кликом и ставьте кликами по клеткам."))
            .small()
            .color(style::MUTED),
    );
    ui.separator();
    let q = app.pattern_ed.search.to_lowercase();
    let items: Vec<(String, String, Option<[u8; 3]>)> = app
        .entries
        .iter()
        .filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q))
        .map(|e| (e.id.clone(), e.name.clone(), app.meta.get(&e.id).color))
        .collect();
    egui::ScrollArea::vertical().id_salt("pattern_palette_list").auto_shrink(false).show(ui, |ui| {
        for (id, name, color) in items {
            let selected = app.pattern_ed.brush.as_deref() == Some(id.as_str());
            // Своя строка: и клик (выбрать кисть), и перетаскивание на дорожку.
            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::click_and_drag());
            let painter = ui.painter();
            if selected {
                painter.rect_filled(rect, 4.0, ui.visuals().selection.bg_fill);
            } else if resp.hovered() {
                painter.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
            }
            let dot_color = color.map(widgets::rgb).unwrap_or(style::MUTED);
            painter.circle_filled(egui::pos2(rect.left() + 8.0, rect.center().y), 4.0, dot_color);
            let text_color = if selected { ui.visuals().selection.stroke.color } else { ui.visuals().text_color() };
            painter.with_clip_rect(rect).text(
                egui::pos2(rect.left() + 18.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                &name,
                egui::TextStyle::Body.resolve(ui.style()),
                text_color,
            );
            if resp.dragged() {
                resp.dnd_set_drag_payload(id.clone());
                // Подпись звука едет за мышью.
                if let Some(pos) = ui.ctx().pointer_interact_pos() {
                    let layer = ui.ctx().layer_painter(egui::LayerId::new(
                        egui::Order::Tooltip,
                        egui::Id::unique("pattern_drag_preview"),
                    ));
                    let r = egui::Rect::from_min_size(pos + egui::vec2(10.0, 4.0), egui::vec2(140.0, 22.0));
                    layer.rect_filled(r, 4.0, dot_color.gamma_multiply(0.8));
                    layer.with_clip_rect(r).text(
                        r.left_center() + egui::vec2(6.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        &name,
                        egui::FontId::proportional(12.0),
                        Color32::WHITE,
                    );
                }
            }
            if resp.clicked() {
                app.pattern_ed.brush = if selected { None } else { Some(id.clone()) };
            }
            if resp.double_clicked() {
                app.pattern_ed.brush = Some(id.clone());
                app.player.play(&id, Some(zv_core::settings::PlayTarget::Monitor));
            }
            resp.on_hover_text(tr(
                "Клик — выбрать для расстановки, двойной клик — прослушать, перетащить — на дорожку",
            ));
        }
    });
}

fn toolbar(app: &mut ZvApp, ui: &mut egui::Ui, dir: &std::path::Path) {
    let playing = is_playing(app);
    ui.horizontal_wrapped(|ui| {
        let (label, color) =
            if playing { (tr("■ Стоп"), style::DANGER) } else { (tr("▶ Играть"), style::ACCENT) };
        if ui
            .add(egui::Button::new(RichText::new(label).strong().color(color)).min_size(egui::vec2(90.0, 26.0)))
            .on_hover_text(tr("Пробел — играть/стоп"))
            .clicked()
        {
            if playing {
                stop(app);
            } else if !play(app, false) {
                app.toast(tr("Звуки ещё загружаются — попробуйте через секунду"));
            }
        }
        let ed = &mut app.pattern_ed;
        if ui.checkbox(&mut ed.pattern.looping, tr("По кругу")).changed() {
            ed.mark();
        }
        ui.separator();
        ui.label("BPM");
        if ui.add(egui::DragValue::new(&mut ed.pattern.bpm).range(40.0..=300.0).speed(0.5).fixed_decimals(0)).changed()
        {
            ed.mark();
        }
        ui.label(tr("Тактов:"));
        if ui.add(egui::DragValue::new(&mut ed.pattern.bars).range(1..=64)).changed() {
            ed.mark();
        }
        ui.label(tr("Долей в такте:"));
        if ui.add(egui::DragValue::new(&mut ed.pattern.beats_per_bar).range(2..=12)).changed() {
            ed.mark();
        }
        ui.label(tr("Сетка:"));
        let cur =
            GRID_STEPS.iter().find(|(s, _)| (*s - ed.pattern.grid).abs() < 1e-9).map(|(_, l)| tr(l)).unwrap_or("?");
        egui::ComboBox::from_id_salt("pattern_grid").selected_text(cur).width(90.0).show_ui(ui, |ui| {
            for (step, label) in GRID_STEPS {
                if ui.selectable_label((step - ed.pattern.grid).abs() < 1e-9, tr(label)).clicked() {
                    ed.pattern.grid = step;
                    ed.mark();
                }
            }
        });
        ui.label(tr("Масштаб:"));
        ui.add(egui::Slider::new(&mut ed.zoom, 20.0..=200.0).show_value(false));
    });
    ui.horizontal_wrapped(|ui| {
        // Имя и список паттернов.
        ui.label(tr("Паттерн:"));
        let resp = ui.add(egui::TextEdit::singleline(&mut app.pattern_ed.name_buf).desired_width(160.0));
        if resp.lost_focus() {
            let n = app.pattern_ed.name_buf.trim().to_string();
            if !n.is_empty() && n != app.pattern_ed.pattern.name {
                app.pattern_ed.pattern.name = n;
                app.pattern_ed.mark();
            } else {
                app.pattern_ed.name_buf = app.pattern_ed.pattern.name.clone();
            }
        }
        let saved = list_patterns(dir);
        let mut open: Option<String> = None;
        egui::ComboBox::from_id_salt("pattern_open").selected_text(tr("📂 Открыть")).width(120.0).show_ui(ui, |ui| {
            if saved.is_empty() {
                ui.label(RichText::new(tr("Сохранённых паттернов нет")).color(style::MUTED));
            }
            for n in &saved {
                if ui.selectable_label(*n == app.pattern_ed.pattern.name, n).clicked() {
                    open = Some(n.clone());
                }
            }
        });
        if ui.button(tr("➕ Новый")).clicked() {
            app.pattern_ed.save_if_due(dir, true);
            stop(app);
            let name = new_name(dir);
            let ed = &mut app.pattern_ed;
            ed.pattern = Pattern { name: name.clone(), ..Default::default() };
            ed.saved_name.clear();
            ed.name_buf = name;
            ed.selected = None;
            ed.mark();
        }
        if ui
            .button(RichText::new(tr("🗑 Удалить паттерн")).color(style::DANGER))
            .on_hover_text(tr("Файл паттерна уйдёт в Корзину"))
            .clicked()
        {
            stop(app);
            let f = file_for(dir, &app.pattern_ed.pattern.name);
            if f.exists() {
                let _ = crate::sysutil::delete_to_recycle_bin(&f);
            }
            app.pattern_ed.dirty = None;
            app.pattern_ed.loaded = false;
            app.pattern_ed.load_initial(dir);
        }
        ui.separator();
        if ui.button(tr("➕ Дорожка")).clicked() {
            app.pattern_ed.pattern.tracks.push(Track::default());
            app.pattern_ed.mark();
        }
        if ui
            .button(RichText::new(tr("💾 Сохранить как звук")).color(style::ACCENT))
            .on_hover_text(tr("Свести в WAV и добавить в библиотеку (папка «Паттерны»)"))
            .clicked()
        {
            export(app);
        }
        if let Some(n) = open {
            app.pattern_ed.save_if_due(dir, true);
            stop(app);
            app.pattern_ed.open(dir, &n);
        }
    });
    if !app.pattern_ed.missing.is_empty() {
        ui.label(
            RichText::new(tf!("Не найдены звуки (удалены?): {}", app.pattern_ed.missing.join(", ")))
                .small()
                .color(style::DANGER),
        );
    }
}

/// Цвет звука на дорожке: свой цвет звука или постоянный цвет по имени.
fn clip_color(app: &ZvApp, id: &str) -> Color32 {
    if let Some(c) = app.meta.get(id).color {
        return widgets::rgb(c);
    }
    let h = id.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    widgets::rgb(widgets::PALETTE[(h as usize) % widgets::PALETTE.len()])
}

fn timeline(app: &mut ZvApp, ui: &mut egui::Ui) {
    let p = app.pattern_ed.pattern.clone();
    let zoom = app.pattern_ed.zoom;
    let len_beats = p.length_beats();
    let n_tracks = p.tracks.len();

    ui.horizontal_top(|ui| {
        // --- Заголовки дорожек ---
        ui.vertical(|ui| {
            ui.set_width(HEADER_W);
            // Без промежутков — строки заголовков должны совпадать с дорожками справа.
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.allocate_exact_size(egui::vec2(HEADER_W, RULER_H), egui::Sense::hover());
            let mut remove: Option<usize> = None;
            for (i, t) in app.pattern_ed.pattern.tracks.iter_mut().enumerate() {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(HEADER_W, LANE_H), egui::Sense::hover());
                let mut changed = false;
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(3.0)), |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.horizontal(|ui| {
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut t.name)
                                    .desired_width(92.0)
                                    .hint_text(tf!("Дорожка {}", i + 1)),
                            )
                            .changed();
                        let m =
                            if t.muted { RichText::new("M").strong().color(style::DANGER) } else { RichText::new("M") };
                        if ui.small_button(m).on_hover_text(tr("Выключить дорожку")).clicked() {
                            t.muted = !t.muted;
                            changed = true;
                        }
                        if ui.small_button("✖").on_hover_text(tr("Удалить дорожку")).clicked() {
                            remove = Some(i);
                        }
                    });
                    changed |= ui
                        .add(egui::Slider::new(&mut t.volume, 0.0..=1.5).show_value(false))
                        .on_hover_text(tf!("Громкость дорожки: {} %", (t.volume * 100.0).round()))
                        .changed();
                });
                if changed {
                    app.pattern_ed.dirty = Some(Instant::now());
                    app.pattern_ed.render_needed = true;
                }
            }
            if let Some(i) = remove {
                app.pattern_ed.pattern.tracks.remove(i);
                if app.pattern_ed.pattern.tracks.is_empty() {
                    app.pattern_ed.pattern.tracks.push(Track::default());
                }
                app.pattern_ed.selected = None;
                app.pattern_ed.mark();
            }
        });

        // --- Сетка с клипами ---
        egui::ScrollArea::horizontal().id_salt("pattern_h").auto_shrink([false, true]).show(ui, |ui| {
            let width = (len_beats as f32 * zoom + 40.0).max(ui.available_width());
            let height = RULER_H + n_tracks as f32 * LANE_H;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            let x_of = |beat: f64| rect.left() + beat as f32 * zoom;
            let beat_of = |x: f32| ((x - rect.left()) / zoom) as f64;
            let lane_of = |y: f32| ((y - rect.top() - RULER_H) / LANE_H).floor() as i64;
            let lane_rect = |i: usize| {
                egui::Rect::from_min_size(
                    egui::pos2(rect.left(), rect.top() + RULER_H + i as f32 * LANE_H),
                    egui::vec2(width, LANE_H),
                )
            };

            // Фон, клетки сетки, такты.
            painter.rect_filled(rect, 0.0, Color32::from_rgb(22, 23, 29));
            for i in 0..n_tracks {
                let r = lane_rect(i);
                let fill = if i % 2 == 0 { Color32::from_rgb(30, 31, 39) } else { Color32::from_rgb(26, 27, 34) };
                painter.rect_filled(r, 0.0, fill);
            }
            let end_x = x_of(len_beats);
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(end_x, rect.top()), rect.right_bottom()),
                0.0,
                Color32::from_black_alpha(90),
            );
            let step = if p.grid > 0.0 { p.grid } else { 1.0 };
            let mut b = 0.0;
            while b <= len_beats + 1e-9 {
                let x = x_of(b);
                let is_bar = (b / p.beats_per_bar as f64).fract().abs() < 1e-9;
                let is_beat = b.fract().abs() < 1e-9;
                let color = if is_bar {
                    Color32::from_gray(110)
                } else if is_beat {
                    Color32::from_gray(65)
                } else {
                    Color32::from_gray(42)
                };
                painter.line_segment([egui::pos2(x, rect.top() + RULER_H), egui::pos2(x, rect.bottom())], (1.0, color));
                if is_bar {
                    let bar = (b / p.beats_per_bar as f64).round() as u32 + 1;
                    painter.text(
                        egui::pos2(x + 3.0, rect.top() + 3.0),
                        egui::Align2::LEFT_TOP,
                        bar.to_string(),
                        egui::FontId::monospace(11.0),
                        style::MUTED,
                    );
                }
                b += step;
                if step * zoom as f64 / 1.0 < 4.0 {
                    break; // слишком мелко — не рисуем частые линии
                }
            }

            // Клипы.
            let mut hits: Vec<(egui::Rect, usize, usize)> = Vec::new();
            for (ti, t) in p.tracks.iter().enumerate() {
                for (ci, c) in t.clips.iter().enumerate() {
                    let beats = app
                        .player
                        .cached_data(&c.sound)
                        .map(|d| pattern_render::clip_beats(&p, &d, c.speed()))
                        .or_else(|| app.player.duration(&c.sound).map(|d| d / c.speed() / p.beat_secs()))
                        .unwrap_or(p.grid.max(0.25));
                    if app.player.cached_data(&c.sound).is_none() && app.entry(&c.sound).is_some() {
                        app.player.preload(&c.sound);
                    }
                    let lr = lane_rect(ti).shrink2(egui::vec2(0.0, 4.0));
                    let r = egui::Rect::from_min_max(
                        egui::pos2(x_of(c.start), lr.top()),
                        egui::pos2(x_of(c.start + beats).max(x_of(c.start) + 6.0), lr.bottom()),
                    );
                    hits.push((r, ti, ci));
                    let exists = app.entry(&c.sound).is_some();
                    let base = if exists { clip_color(app, &c.sound) } else { style::DANGER };
                    let selected = app.pattern_ed.selected == Some((ti, ci));
                    let fill = if t.muted { base.gamma_multiply(0.25) } else { base.gamma_multiply(0.65) };
                    painter.rect_filled(r, 4.0, fill);
                    painter.rect_stroke(
                        r,
                        4.0,
                        egui::Stroke::new(
                            if selected { 2.0 } else { 1.0 },
                            if selected { Color32::WHITE } else { base },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    let name =
                        app.entry(&c.sound).map(|e| e.name.clone()).unwrap_or_else(|| tr("(звук удалён)").into());
                    let mut label = name;
                    if c.pitch.abs() > 0.01 {
                        label = format!("{label}  {:+}", c.pitch.round() as i32);
                    }
                    let clip_painter = painter.with_clip_rect(r.shrink(2.0).intersect(rect));
                    clip_painter.text(
                        r.left_center() + egui::vec2(5.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        label,
                        egui::FontId::proportional(12.0),
                        Color32::WHITE,
                    );
                }
            }

            // Бегунок.
            if let Some((secs, _)) = app.player.position(KEY) {
                let beat = secs / p.beat_secs();
                let x = x_of(if p.looping { beat % len_beats } else { beat });
                painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], (2.0, Color32::WHITE));
            }

            // --- Мышь ---
            let hit_at = |pos: egui::Pos2| hits.iter().rev().find(|(r, ..)| r.contains(pos)).map(|(_, t, c)| (*t, *c));
            let pointer = resp.interact_pointer_pos().or(resp.hover_pos());

            // Где нажали кнопку (а не где мышь уже оказалась, когда началось перетаскивание).
            if resp.drag_started()
                && let Some(pos) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos())
                && let Some((t, c)) = hit_at(pos)
            {
                let start = p.tracks[t].clips[c].start;
                app.pattern_ed.drag = Some(Drag { track: t, clip: c, grab: beat_of(pos.x) - start });
                app.pattern_ed.selected = Some((t, c));
            }
            if let (Some(d), Some(pos)) = (app.pattern_ed.drag.as_ref(), pointer)
                && resp.dragged()
            {
                let (t0, c0, grab) = (d.track, d.clip, d.grab);
                let new_start = app.pattern_ed.pattern.snap(beat_of(pos.x) - grab);
                let lane = lane_of(pos.y).clamp(0, n_tracks as i64 - 1) as usize;
                let ed = &mut app.pattern_ed;
                if lane != t0 {
                    let mut clip = ed.pattern.tracks[t0].clips.remove(c0);
                    clip.start = new_start;
                    ed.pattern.tracks[lane].clips.push(clip);
                    let ci = ed.pattern.tracks[lane].clips.len() - 1;
                    ed.drag = Some(Drag { track: lane, clip: ci, grab });
                    ed.selected = Some((lane, ci));
                    ed.mark();
                } else if let Some(clip) = ed.clip_mut((t0, c0))
                    && (clip.start - new_start).abs() > 1e-9
                {
                    clip.start = new_start;
                    ed.mark();
                }
            }
            if resp.drag_stopped() {
                app.pattern_ed.drag = None;
            }

            if resp.clicked()
                && let Some(pos) = resp.interact_pointer_pos()
            {
                if pos.y < rect.top() + RULER_H {
                    // Клик по линейке — перемотка.
                    if is_playing(app) {
                        app.player.seek(KEY, beat_of(pos.x).clamp(0.0, len_beats) * p.beat_secs());
                    }
                } else if let Some(hit) = hit_at(pos) {
                    app.pattern_ed.selected = Some(hit);
                } else if let Some(brush) = app.pattern_ed.brush.clone() {
                    let lane = lane_of(pos.y);
                    if lane >= 0 && (lane as usize) < n_tracks {
                        let ed = &mut app.pattern_ed;
                        let start = ed.pattern.snap_floor(beat_of(pos.x));
                        ed.pattern.tracks[lane as usize].clips.push(Clip { sound: brush, start, ..Default::default() });
                        ed.selected = Some((lane as usize, ed.pattern.tracks[lane as usize].clips.len() - 1));
                        ed.mark();
                    }
                } else {
                    app.pattern_ed.selected = None;
                }
            }

            // Перетаскивание звука из списка.
            if let Some(id) = resp.dnd_release_payload::<String>()
                && let Some(pos) = ui.ctx().pointer_interact_pos()
            {
                let lane = lane_of(pos.y);
                if lane >= 0 && (lane as usize) < n_tracks {
                    let ed = &mut app.pattern_ed;
                    let start = ed.pattern.snap_floor(beat_of(pos.x));
                    ed.pattern.tracks[lane as usize].clips.push(Clip {
                        sound: (*id).clone(),
                        start,
                        ..Default::default()
                    });
                    ed.selected = Some((lane as usize, ed.pattern.tracks[lane as usize].clips.len() - 1));
                    ed.brush = Some((*id).clone());
                    ed.mark();
                }
            }
            if resp.dnd_hover_payload::<String>().is_some()
                && let Some(pos) = ui.ctx().pointer_hover_pos()
            {
                let lane = lane_of(pos.y);
                if lane >= 0 && (lane as usize) < n_tracks {
                    let x = x_of(app.pattern_ed.pattern.snap_floor(beat_of(pos.x)));
                    let lr = lane_rect(lane as usize);
                    painter.line_segment([egui::pos2(x, lr.top()), egui::pos2(x, lr.bottom())], (2.0, style::ACCENT));
                }
            }

            // Правый клик по звуку — меню.
            if resp.secondary_clicked() {
                app.pattern_ed.menu_clip = resp.interact_pointer_pos().and_then(hit_at);
                if let Some(h) = app.pattern_ed.menu_clip {
                    app.pattern_ed.selected = Some(h);
                }
            }
            let menu_target = app.pattern_ed.menu_clip;
            resp.context_menu(|ui| {
                let Some(target) = menu_target else {
                    ui.label(RichText::new(tr("Правый клик по звуку — его меню")).color(style::MUTED));
                    return;
                };
                if ui.button(tr("⧉ Копия следом  (Ctrl+D)")).clicked() {
                    duplicate(app, target);
                    ui.close();
                }
                if ui.button(tr("↺ Сбросить высоту и громкость")).clicked() {
                    if let Some(c) = app.pattern_ed.clip_mut(target) {
                        c.pitch = 0.0;
                        c.volume = 1.0;
                    }
                    app.pattern_ed.mark();
                    ui.close();
                }
                if ui.button(tr("🎨 Взять как кисть")).clicked() {
                    app.pattern_ed.brush = app.pattern_ed.clip_mut(target).map(|c| c.sound.clone());
                    ui.close();
                }
                ui.separator();
                if ui.button(RichText::new(tr("🗑 Удалить  (Del)")).color(style::DANGER)).clicked() {
                    delete(app, target);
                    ui.close();
                }
            });

            if app.pattern_ed.drag.is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if resp.hover_pos().and_then(hit_at).is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
        });
    });
}

fn duplicate(app: &mut ZvApp, (t, c): (usize, usize)) {
    let p = app.pattern_ed.pattern.clone();
    let Some(clip) = p.tracks.get(t).and_then(|tr| tr.clips.get(c)).cloned() else { return };
    let beats =
        app.player.cached_data(&clip.sound).map(|d| pattern_render::clip_beats(&p, &d, clip.speed())).unwrap_or(1.0);
    // Ставим сразу после звука, по сетке вверх.
    let mut start = clip.start + beats;
    if p.grid > 0.0 {
        start = ((start / p.grid) - 1e-9).ceil() * p.grid;
    }
    let ed = &mut app.pattern_ed;
    ed.pattern.tracks[t].clips.push(Clip { start, ..clip });
    ed.selected = Some((t, ed.pattern.tracks[t].clips.len() - 1));
    ed.mark();
}

fn delete(app: &mut ZvApp, (t, c): (usize, usize)) {
    let ed = &mut app.pattern_ed;
    if let Some(track) = ed.pattern.tracks.get_mut(t)
        && c < track.clips.len()
    {
        track.clips.remove(c);
        ed.selected = None;
        ed.mark();
    }
}

fn inspector(app: &mut ZvApp, ui: &mut egui::Ui) {
    let Some(sel) = app.pattern_ed.selected else {
        let hint = match &app.pattern_ed.brush {
            Some(b) => tf!(
                "Кисть: «{}» — кликайте по клеткам дорожек, чтобы ставить звук.",
                app.entry(b).map(|e| e.name.clone()).unwrap_or_default()
            ),
            None => tr("Выберите звук слева (кисть) или перетащите его на дорожку. Выбранный звук на дорожке — здесь его высота и громкость.").to_string(),
        };
        ui.label(RichText::new(hint).color(style::MUTED));
        return;
    };
    let name = app
        .pattern_ed
        .pattern
        .tracks
        .get(sel.0)
        .and_then(|t| t.clips.get(sel.1))
        .map(|c| app.entry(&c.sound).map(|e| e.name.clone()).unwrap_or_else(|| tr("(звук удалён)").into()));
    let Some(name) = name else {
        app.pattern_ed.selected = None;
        return;
    };
    let mut changed = false;
    let mut del = false;
    let mut dup = false;
    let beat_secs = app.pattern_ed.pattern.beat_secs();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(name).strong());
        if let Some(c) = app.pattern_ed.clip_mut(sel) {
            ui.label(
                RichText::new(tf!(
                    "начало: доля {} ({})",
                    format!("{:.2}", c.start + 1.0),
                    widgets::fmt_time(c.start * beat_secs)
                ))
                .color(style::MUTED),
            );
            ui.separator();
            ui.label(tr("Высота:"));
            changed |= ui
                .add(egui::Slider::new(&mut c.pitch, -12.0..=12.0).step_by(1.0).suffix(tr(" пт")))
                .on_hover_text(tr("В полутонах. Выше — быстрее и короче, ниже — медленнее и длиннее."))
                .changed();
            ui.label(tr("Громкость:"));
            changed |= widgets::volume_slider(ui, &mut c.volume, 2.0).changed();
        }
        if ui.button(tr("⧉ Копия")).clicked() {
            dup = true;
        }
        if ui.button(RichText::new(tr("🗑 Удалить")).color(style::DANGER)).clicked() {
            del = true;
        }
    });
    if changed {
        app.pattern_ed.mark();
    }
    if dup {
        duplicate(app, sel);
    }
    if del {
        delete(app, sel);
    }
}

fn keyboard(app: &mut ZvApp, ui: &mut egui::Ui) {
    // Не мешаем вводу текста.
    if ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let (del, dup, space, esc) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::Delete),
            i.modifiers.ctrl && i.key_pressed(egui::Key::D),
            i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::Escape),
        )
    });
    if let Some(sel) = app.pattern_ed.selected {
        if del {
            delete(app, sel);
        }
        if dup {
            duplicate(app, sel);
        }
    }
    if esc {
        app.pattern_ed.selected = None;
        app.pattern_ed.brush = None;
    }
    if space {
        if is_playing(app) {
            stop(app);
        } else {
            play(app, false);
        }
    }
}
