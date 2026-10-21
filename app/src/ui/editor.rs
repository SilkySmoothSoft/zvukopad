//! Мини-редактор: обрезка звука по волне.
//!
//! * Волна звука; начало и конец фрагмента перетаскиваются мышью за маркеры.
//! * Протянуть мышью по пустому месту — выделить новый фрагмент.
//! * Плавное появление/затухание на краях, чтобы не было щелчков.
//! * «Сохранить как новый» — отдельный файл; «Заменить оригинал» — оригинал уходит в Корзину,
//!   горячая клавиша и громкость переносятся на новый файл.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use zv_core::i18n::tr;
use zv_core::tf;

use crossbeam_channel::Receiver;
use eframe::egui::{self, Color32, RichText, Stroke};
use zv_audio::SoundData;
use zv_core::settings::PlayTarget;

use super::{ZvApp, style, widgets};
use crate::sysutil;

/// Сколько отрезков волны считать заранее (дальше рисуется с прореживанием).
const PEAK_BUCKETS: usize = 4000;
const PREVIEW_KEY: &str = "__editor__";

pub struct EditorState {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    loading: Option<Receiver<Result<SoundData, String>>>,
    data: Option<Arc<SoundData>>,
    error: Option<String>,
    peaks: Vec<(f32, f32)>,
    /// Границы фрагмента, секунды.
    start: f64,
    end: f64,
    fade_ms: f32,
    new_name: String,
    /// Какой маркер тащим.
    drag: Option<Handle>,
    drag_anchor: f64,
    /// Для бегунка при прослушивании: когда и с какого места начали.
    preview: Option<(Instant, f64, f64)>,
    confirm_replace: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Handle {
    Start,
    End,
    NewRange,
}

impl EditorState {
    pub fn open(id: &str, name: &str, path: PathBuf, repaint: egui::Context) -> Self {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let p = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(zv_audio::decode_file(&p));
            repaint.request_repaint();
        });
        Self {
            id: id.to_string(),
            name: name.to_string(),
            path,
            loading: Some(rx),
            data: None,
            error: None,
            peaks: Vec::new(),
            start: 0.0,
            end: 0.0,
            fade_ms: 10.0,
            new_name: tf!("{} (фрагмент)", name),
            drag: None,
            drag_anchor: 0.0,
            preview: None,
            confirm_replace: false,
        }
    }

    fn duration(&self) -> f64 {
        self.data.as_ref().map(|d| d.duration_secs()).unwrap_or(0.0)
    }

    fn poll(&mut self) {
        let Some(rx) = &self.loading else { return };
        if let Ok(res) = rx.try_recv() {
            self.loading = None;
            match res {
                Ok(d) => {
                    self.peaks = d.peaks(PEAK_BUCKETS);
                    self.end = d.duration_secs();
                    self.data = Some(Arc::new(d));
                }
                Err(e) => self.error = Some(e),
            }
        }
    }

    /// Выбранный кусок с плавными краями.
    fn fragment(&self) -> Option<SoundData> {
        let d = self.data.as_ref()?;
        let rate = d.rate as f64;
        let a = (self.start * rate).round() as usize;
        let b = (self.end * rate).round() as usize;
        if b <= a + 1 {
            return None;
        }
        let fade = (self.fade_ms as f64 / 1000.0 * rate) as usize;
        Some(d.slice(a, b, fade))
    }
}

pub fn show(app: &mut ZvApp, ctx: &egui::Context) {
    let Some(mut ed) = app.editor.take() else { return };
    ed.poll();
    let mut keep_open = true;
    let screen = ctx.content_rect();

    egui::Modal::new(egui::Id::unique("editor")).show(ctx, |ui| {
        ui.set_width((screen.width() - 80.0).clamp(420.0, 1000.0));
        ui.horizontal(|ui| {
            ui.heading(tf!("✂ Обрезка: {}", ed.name));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Закрыть")).clicked() {
                    keep_open = false;
                }
            });
        });
        ui.add_space(6.0);

        if let Some(e) = &ed.error {
            ui.label(RichText::new(tf!("Не удалось открыть звук: {}", e)).color(style::DANGER));
            return;
        }
        if ed.data.is_none() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr("Загружаю звук…"));
            });
            return;
        }

        ui.label(
            RichText::new(tr(
                "Тяните белые маркеры или проведите мышью по волне, чтобы выделить фрагмент. Esc — закрыть.",
            ))
            .small()
            .color(style::MUTED),
        );
        waveform(&mut ed, ui);
        ui.add_space(6.0);
        controls(app, &mut ed, ui, &mut keep_open);
    });

    if ui_escape(ctx) && ed.drag.is_none() {
        keep_open = false;
    }
    if keep_open {
        app.editor = Some(ed);
    } else {
        app.player.stop(PREVIEW_KEY);
    }
}

fn ui_escape(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !ctx.egui_wants_keyboard_input()
}

fn waveform(ed: &mut EditorState, ui: &mut egui::Ui) {
    let dur = ed.duration().max(1e-6);
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 160.0), egui::Sense::click_and_drag());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 6.0, Color32::from_rgb(22, 23, 30));

    let x_of = |t: f64| rect.left() + (t / dur) as f32 * rect.width();
    let t_of = |x: f32| (((x - rect.left()) / rect.width()) as f64 * dur).clamp(0.0, dur);

    // Волна.
    let mid = rect.center().y;
    let half = rect.height() * 0.45;
    let n = ed.peaks.len().max(1);
    let (sx, ex) = (x_of(ed.start), x_of(ed.end));
    for px in 0..rect.width() as usize {
        let b0 = px * n / rect.width() as usize;
        let b1 = ((px + 1) * n / rect.width() as usize).max(b0 + 1).min(n);
        let (mut lo, mut hi) = (0.0f32, 0.0f32);
        for &(l, h) in &ed.peaks[b0.min(n - 1)..b1] {
            lo = lo.min(l);
            hi = hi.max(h);
        }
        let x = rect.left() + px as f32 + 0.5;
        let inside = x >= sx && x <= ex;
        let color = if inside { style::ACCENT } else { Color32::from_rgb(80, 82, 95) };
        p.line_segment([egui::pos2(x, mid - hi * half), egui::pos2(x, mid - lo * half - 0.5)], Stroke::new(1.0, color));
    }
    p.line_segment(
        [egui::pos2(rect.left(), mid), egui::pos2(rect.right(), mid)],
        Stroke::new(1.0, Color32::from_gray(60)),
    );

    // Затемнение вне фрагмента.
    let shade = Color32::from_black_alpha(110);
    p.rect_filled(egui::Rect::from_min_max(rect.min, egui::pos2(sx, rect.bottom())), 0.0, shade);
    p.rect_filled(egui::Rect::from_min_max(egui::pos2(ex, rect.top()), rect.max), 0.0, shade);

    // Маркеры: линия и «ручка» сверху, за которую удобно тянуть.
    for x in [sx, ex] {
        p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], Stroke::new(2.0, Color32::WHITE));
        let grip = egui::Rect::from_center_size(egui::pos2(x, rect.top() + 9.0), egui::vec2(10.0, 18.0));
        p.rect_filled(grip, 3.0, Color32::WHITE);
    }

    // Бегунок прослушивания.
    if let Some((t0, from, to)) = ed.preview {
        let pos = from + t0.elapsed().as_secs_f64();
        if pos <= to {
            let x = x_of(pos);
            p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], Stroke::new(1.5, style::PLAYING));
            ui.ctx().request_repaint();
        } else {
            ed.preview = None;
        }
    }

    // Перетаскивание.
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.drag_started() || (resp.clicked() && ed.drag.is_none()) {
            let near = |x: f32| (pos.x - x).abs() < 10.0;
            ed.drag = Some(if near(sx) {
                Handle::Start
            } else if near(ex) {
                Handle::End
            } else {
                ed.drag_anchor = t_of(pos.x);
                Handle::NewRange
            });
        }
        let t = t_of(pos.x);
        match ed.drag {
            Some(Handle::Start) => ed.start = t.min(ed.end - 0.01).max(0.0),
            Some(Handle::End) => ed.end = t.max(ed.start + 0.01).min(dur),
            Some(Handle::NewRange) if resp.dragged() => {
                ed.start = ed.drag_anchor.min(t);
                ed.end = ed.drag_anchor.max(t).max(ed.start + 0.01).min(dur);
            }
            _ => {}
        }
    }
    if resp.drag_stopped() || resp.clicked() {
        ed.drag = None;
    }
    let hover_handle = resp.hover_pos().is_some_and(|h| (h.x - sx).abs() < 10.0 || (h.x - ex).abs() < 10.0);
    if hover_handle || matches!(ed.drag, Some(Handle::Start | Handle::End)) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
}

fn controls(app: &mut ZvApp, ed: &mut EditorState, ui: &mut egui::Ui, keep_open: &mut bool) {
    let dur = ed.duration();
    ui.horizontal(|ui| {
        ui.label(tr("Начало:"));
        ui.add(
            egui::DragValue::new(&mut ed.start)
                .range(0.0..=ed.end - 0.01)
                .speed(0.01)
                .suffix(tr(" с"))
                .fixed_decimals(2),
        );
        ui.label(tr("Конец:"));
        ui.add(
            egui::DragValue::new(&mut ed.end)
                .range(ed.start + 0.01..=dur)
                .speed(0.01)
                .suffix(tr(" с"))
                .fixed_decimals(2),
        );
        ui.label(
            RichText::new(tf!("Длина фрагмента: {}", widgets::fmt_duration(ed.end - ed.start))).color(style::MUTED),
        );
        if ui.button(tr("Выделить всё")).clicked() {
            ed.start = 0.0;
            ed.end = dur;
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("Плавные края:"));
        ui.add(egui::Slider::new(&mut ed.fade_ms, 0.0..=1000.0).suffix(tr(" мс")).logarithmic(true))
            .on_hover_text(tr("Плавное появление и затухание, чтобы на краях не было щелчков"));
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button(tr("▶ Прослушать фрагмент")).on_hover_text(tr("Только себе")).clicked()
            && let Some(frag) = ed.fragment()
        {
            app.player.play_adhoc(PREVIEW_KEY, Arc::new(frag), PlayTarget::Monitor);
            ed.preview = Some((Instant::now(), ed.start, ed.end));
        }
        if ui.button(tr("■ Стоп")).clicked() {
            app.player.stop(PREVIEW_KEY);
            ed.preview = None;
        }
    });
    ui.separator();

    ui.horizontal(|ui| {
        ui.label(tr("Название нового звука:"));
        ui.add(egui::TextEdit::singleline(&mut ed.new_name).desired_width(260.0).char_limit(80));
        if ui.button(tr("💾 Сохранить как новый")).clicked() {
            save_as_new(app, ed, keep_open);
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut ed.confirm_replace, "");
        ui.add_enabled_ui(ed.confirm_replace, |ui| {
            if ui
                .button(RichText::new(tr("Заменить оригинал")).color(style::DANGER))
                .on_hover_text(tr("Оригинал уйдёт в Корзину; горячая клавиша и громкость сохранятся"))
                .clicked()
            {
                replace_original(app, ed, keep_open);
            }
        });
        ui.label(
            RichText::new(tr("← поставьте галочку, чтобы заменить исходный файл (он уйдёт в Корзину)"))
                .small()
                .color(style::MUTED),
        );
    });
}

fn save_as_new(app: &mut ZvApp, ed: &EditorState, keep_open: &mut bool) {
    let Some(frag) = ed.fragment() else {
        app.toast_error(tr("Фрагмент слишком короткий"));
        return;
    };
    let dir = ed.path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| app.sounds_dir.clone());
    let stem = zv_core::library::sanitize_file_name(&ed.new_name);
    let path = zv_core::library::unique_path(&dir, &stem, "wav");
    match frag.write_wav(&path) {
        Ok(()) => {
            log::info!("Фрагмент сохранён: {}", path.display());
            app.rescan();
            let id = zv_core::library::id_for(&app.sounds_dir, &path);
            app.select(id);
            app.toast(tf!("Сохранено: «{}»", stem));
            *keep_open = false;
        }
        Err(e) => app.toast_error(e),
    }
}

fn replace_original(app: &mut ZvApp, ed: &EditorState, keep_open: &mut bool) {
    let Some(frag) = ed.fragment() else {
        app.toast_error(tr("Фрагмент слишком короткий"));
        return;
    };
    app.player.stop(&ed.id);
    let dir = ed.path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| app.sounds_dir.clone());
    let stem = ed.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| tr("звук").into());
    // Сначала пишем новый файл рядом, потом убираем оригинал, потом ставим новый на его место.
    let tmp = dir.join(format!("_{stem}.zvnew.wav"));
    if let Err(e) = frag.write_wav(&tmp) {
        app.toast_error(e);
        return;
    }
    if let Err(e) = sysutil::delete_to_recycle_bin(&ed.path) {
        let _ = std::fs::remove_file(&tmp);
        app.toast_error(tf!("Не удалось убрать оригинал: {}", e));
        return;
    }
    let mut target = dir.join(format!("{stem}.wav"));
    if target.exists() {
        target = zv_core::library::unique_path(&dir, &stem, "wav");
    }
    if let Err(e) = std::fs::rename(&tmp, &target) {
        app.toast_error(tf!("Не удалось сохранить: {}", e));
        return;
    }
    log::info!("Звук обрезан: {} → {}", ed.path.display(), target.display());
    if let Some(new_id) = zv_core::library::id_for(&app.sounds_dir, &target) {
        app.meta.rename(&ed.id, &new_id);
        app.player.forget(&ed.id);
        app.mark_meta_dirty();
        app.rescan();
        app.select(Some(new_id));
    }
    app.toast(tf!("«{}» обрезан, оригинал в Корзине", ed.name));
    *keep_open = false;
}
