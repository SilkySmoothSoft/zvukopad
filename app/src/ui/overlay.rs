//! Оверлей: прозрачное окно поверх всех окон.
//!
//! * Обычный вид — звуки с клавишами и номерами (список или цветные плитки).
//! * Пока набирается номер на нумпаде — крупно набранные цифры и подходящие звуки.
//!
//! Это обычное отдельное окно Windows: оно не встраивается в игру, не перехватывает
//! её отрисовку и не принимает клики мыши (они проходят насквозь). Поэтому античиты
//! видят его как любое другое окно на рабочем столе.

use eframe::egui::{self, Color32, RichText};
use zv_core::controls::Action;
use zv_core::i18n::tr;
use zv_core::settings::{OverlayLayout, OverlayPosition, OverlayStyle};
use zv_core::tf;

use super::{ZvApp, style, widgets};

struct Item {
    key: String,
    code: Option<u32>,
    name: String,
    color: Option<[u8; 3]>,
    playing: bool,
}

enum Content {
    Sounds { items: Vec<Item>, total: usize },
    Numpad { typed: String, items: Vec<Item>, total: usize, exact: bool, repeat_name: Option<String> },
}

pub fn show(app: &mut ZvApp, ctx: &egui::Context) {
    let st = app.settings.overlay.style.clone();
    let content = build_content(app, &st);
    let header = header_text(app);

    // Размер окна по содержимому.
    let monitor = ctx.input(|i| i.viewport().monitor_size).unwrap_or(egui::vec2(1920.0, 1080.0));
    let width = (st.width.clamp(0.08, 1.0) * monitor.x).max(220.0);
    let fs = st.font_size;
    let row_h = fs * 1.65;
    let cols = st.columns.max(1);
    let tile_h = fs * 3.7 + 10.0;
    let body_h = match &content {
        Content::Sounds { items, total } => {
            let more = usize::from(*total > items.len()) as f32 * row_h;
            let empty = usize::from(items.is_empty()) as f32 * row_h;
            match st.layout {
                OverlayLayout::List => items.len() as f32 * row_h + more + empty,
                OverlayLayout::Grid => items.len().div_ceil(cols) as f32 * (tile_h + 6.0) + more + empty,
            }
        }
        Content::Numpad { items, total, .. } => {
            fs * 2.6 + row_h * (items.len().max(1) as f32 + 2.0 + usize::from(*total > items.len()) as f32)
        }
    };
    let footer = app.settings.controls.key(Action::StopAll).to_string();
    let height = (row_h * (1.0 + usize::from(!footer.is_empty()) as f32) + body_h + 30.0).min(monitor.y - 40.0);

    let margin = 16.0;
    let pos = match st.position {
        OverlayPosition::TopLeft => egui::pos2(margin, margin),
        OverlayPosition::TopRight => egui::pos2(monitor.x - width - margin, margin),
        OverlayPosition::BottomLeft => egui::pos2(margin, monitor.y - height - margin - 40.0),
        OverlayPosition::BottomRight => egui::pos2(monitor.x - width - margin, monitor.y - height - margin - 40.0),
        OverlayPosition::TopCenter => egui::pos2((monitor.x - width) / 2.0, margin),
        OverlayPosition::Center => egui::pos2((monitor.x - width) / 2.0, (monitor.y - height) / 2.0),
    };

    let builder = egui::ViewportBuilder::default()
        .with_title(tf!("{} — оверлей", zv_core::app_title()))
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_mouse_passthrough(true)
        .with_taskbar(false)
        .with_active(false)
        .with_resizable(false)
        .with_has_shadow(false)
        .with_inner_size([width, height])
        .with_position(pos);

    let bg = Color32::from_rgba_unmultiplied(18, 19, 24, (st.opacity.clamp(0.0, 1.0) * 255.0) as u8);
    let modifier = app.settings.numpad.modifier.label();
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("zv_overlay"), builder, |ui, _class| {
        egui::CentralPanel::no_frame().show(ui, |ui| {
            egui::Frame::NONE.fill(bg).corner_radius(10.0).inner_margin(10.0).show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                let font = egui::FontId::proportional(fs);
                let mono = egui::FontId::monospace(fs * 0.9);
                ui.label(RichText::new(&header).font(font.clone()).color(style::ACCENT).strong());
                match &content {
                    Content::Sounds { items, total } => {
                        if items.is_empty() {
                            ui.label(
                                RichText::new(tr("Нет звуков с клавишами или номерами"))
                                    .font(font.clone())
                                    .color(style::MUTED),
                            );
                        }
                        match st.layout {
                            OverlayLayout::List => list(ui, items, &font, &mono, &st),
                            OverlayLayout::Grid => grid(ui, items, fs, cols, tile_h, &st),
                        }
                        if *total > items.len() {
                            ui.label(
                                RichText::new(tf!("… и ещё {}", total - items.len()))
                                    .font(font.clone())
                                    .color(style::MUTED),
                            );
                        }
                    }
                    Content::Numpad { typed, items, total, exact, repeat_name } => {
                        ui.label(RichText::new(format!("№ {typed}▌")).size(fs * 2.0).strong().color(Color32::WHITE));
                        ui.label(
                            RichText::new(tf!("отпустите {} — играть · точка — отмена", modifier))
                                .font(egui::FontId::proportional(fs * 0.8))
                                .color(style::MUTED),
                        );
                        if let Some(name) = repeat_name {
                            ui.label(
                                RichText::new(tf!("↻ повторить: {}", name)).font(font.clone()).color(style::PLAYING),
                            );
                        } else if items.is_empty() {
                            ui.label(
                                RichText::new(tr("Нет звуков с таким номером")).font(font.clone()).color(style::DANGER),
                            );
                        }
                        for (i, it) in items.iter().enumerate() {
                            let first_exact = i == 0 && *exact;
                            ui.horizontal(|ui| {
                                dot(ui, it.color, fs);
                                let code = it.code.map(|c| c.to_string()).unwrap_or_default();
                                let mark = if first_exact { "▶" } else { " " };
                                let c = if first_exact { style::PLAYING } else { Color32::from_rgb(200, 200, 210) };
                                ui.label(RichText::new(format!("{mark}{code:>4}")).font(mono.clone()).color(c));
                                let mut name = RichText::new(&it.name).font(font.clone()).color(Color32::WHITE);
                                if first_exact {
                                    name = name.strong().color(style::PLAYING);
                                }
                                ui.add(egui::Label::new(name).truncate());
                            });
                        }
                        if *total > items.len() {
                            ui.label(
                                RichText::new(tf!("… ещё {} — наберите следующую цифру", total - items.len()))
                                    .font(font.clone())
                                    .color(style::MUTED),
                            );
                        }
                    }
                }
                if !footer.is_empty() {
                    ui.label(RichText::new(tf!("{} — стоп", footer)).font(mono.clone()).color(style::MUTED));
                }
            });
        });
    });
}

fn header_text(app: &ZvApp) -> String {
    let mut parts = vec![zv_core::app_title().to_string(), app.settings.audio.target.label().to_string()];
    if app.engine.is_paused() {
        parts.push(tr("⏸ пауза").into());
    }
    if app.settings.effects.enabled {
        parts.push(tr("🎛 эффекты").into());
    }
    if app.engine.replay_info().is_some() {
        parts.push(tr("⏪ буфер").into());
    }
    if app.status.recording.is_some() {
        parts.push(tf!("● запись {}", widgets::fmt_time(app.engine.recorded_seconds())));
    }
    let q = app.queue.snapshot();
    if q.playing
        && let Some(id) = q.current.and_then(|c| q.items.get(c))
    {
        let name = app.entry(id).map(|e| e.name.clone()).unwrap_or_default();
        parts.push(format!("📋 {name}"));
    }
    parts.join(" · ")
}

fn build_content(app: &ZvApp, st: &OverlayStyle) -> Content {
    let typed = app.numpad.buffer();
    let item_for = |id: &str| -> Option<Item> {
        let e = app.entry(id)?;
        let m = app.meta.get(id);
        Some(Item {
            key: m.hotkey.clone().unwrap_or_default(),
            code: m.code,
            name: e.name.clone(),
            color: m.color,
            playing: app.is_playing(id),
        })
    };
    if !typed.is_empty() && app.settings.numpad.enabled {
        let limit = app.settings.numpad.max_hints.max(1);
        let (hits, total) = app.numpad.hints(&typed, limit);
        let exact = hits.first().is_some_and(|(c, _)| c.to_string() == typed);
        let items: Vec<Item> = hits.iter().filter_map(|(_, id)| item_for(id)).collect();
        let repeat_name = (typed == "0")
            .then(|| app.player.last_played().and_then(|id| app.entry(&id).map(|e| e.name.clone())))
            .flatten();
        return Content::Numpad { typed, items, total, exact, repeat_name };
    }
    let numpad_on = app.settings.numpad.enabled && st.show_codes;
    let mut items: Vec<Item> = app
        .entries
        .iter()
        .filter_map(|e| item_for(&e.id))
        .filter(|it| !it.key.is_empty() || (!st.only_with_hotkeys && numpad_on && it.code.is_some()))
        .collect();
    // Клавиши «случайный из тега» — тоже подсказки.
    if app.settings.controls.sound_hotkeys_enabled {
        for (tag, key) in &app.settings.controls.tag_random {
            items.push(Item {
                key: key.clone(),
                code: None,
                name: tf!("🎲 случайный #{}", tag),
                color: None,
                playing: false,
            });
        }
    }
    items.sort_by(|a, b| {
        natural_hotkey_order(&a.key, &b.key).then_with(|| a.code.unwrap_or(u32::MAX).cmp(&b.code.unwrap_or(u32::MAX)))
    });
    let total = items.len();
    items.truncate(st.max_items.max(1));
    if !numpad_on {
        items.iter_mut().for_each(|i| i.code = None);
    }
    Content::Sounds { items, total }
}

fn dot(ui: &mut egui::Ui, color: Option<[u8; 3]>, fs: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(fs * 0.5, fs), egui::Sense::hover());
    if let Some(c) = color {
        ui.painter().circle_filled(rect.center(), fs * 0.25, widgets::rgb(c));
    }
}

fn list(ui: &mut egui::Ui, items: &[Item], font: &egui::FontId, mono: &egui::FontId, st: &OverlayStyle) {
    for it in items {
        ui.horizontal(|ui| {
            dot(ui, it.color, st.font_size);
            let key_color = if it.playing { style::PLAYING } else { Color32::from_rgb(200, 200, 210) };
            let mut left = it.key.clone();
            if let Some(c) = it.code {
                left = if left.is_empty() { format!("№{c}") } else { format!("{left} №{c}") };
            }
            ui.label(RichText::new(format!("{left:<14}")).font(mono.clone()).color(key_color));
            let mut name = RichText::new(&it.name).font(font.clone()).color(Color32::WHITE);
            if it.playing {
                name = name.color(style::PLAYING).strong();
            }
            ui.add(egui::Label::new(name).truncate());
        });
    }
}

fn grid(ui: &mut egui::Ui, items: &[Item], fs: f32, cols: usize, tile_h: f32, st: &OverlayStyle) {
    let gap = 6.0;
    let w = ((ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32).max(40.0);
    for row in items.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for it in row {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(w, tile_h), egui::Sense::hover());
                let base = it.color.map(widgets::rgb).unwrap_or(Color32::from_rgb(60, 62, 75));
                let alpha = (st.opacity.clamp(0.3, 1.0) * 200.0) as u8;
                let fill = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha);
                let p = ui.painter();
                p.rect_filled(rect, 8.0, fill);
                if it.playing {
                    p.rect_stroke(rect, 8.0, egui::Stroke::new(3.0, Color32::WHITE), egui::StrokeKind::Inside);
                }
                let mut top = it.key.clone();
                if let Some(c) = it.code {
                    top = if top.is_empty() { format!("№{c}") } else { format!("{top}  №{c}") };
                }
                p.text(
                    rect.left_top() + egui::vec2(8.0, 6.0),
                    egui::Align2::LEFT_TOP,
                    top,
                    egui::FontId::monospace(fs * 0.75),
                    Color32::from_white_alpha(220),
                );
                // Название — с обрезкой по ширине плитки.
                let galley = p.layout(it.name.clone(), egui::FontId::proportional(fs), Color32::WHITE, w - 16.0);
                let clip = rect.shrink(4.0);
                p.with_clip_rect(clip).galley(
                    rect.left_top() + egui::vec2(8.0, fs * 1.2 + 6.0),
                    galley,
                    Color32::WHITE,
                );
            }
        });
        ui.add_space(gap - ui.spacing().item_spacing.y);
    }
}

/// Сортировка клавиш «по-человечески»: Ctrl+2 раньше Ctrl+10, звуки без клавиш — в конце.
fn natural_hotkey_order(a: &str, b: &str) -> std::cmp::Ordering {
    fn key(s: &str) -> (String, u64, String) {
        let digits_start = s.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        let (prefix, num) = s.split_at(digits_start);
        (prefix.to_lowercase(), num.parse().unwrap_or(0), s.to_lowercase())
    }
    match (a.is_empty(), b.is_empty()) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        (true, true) => std::cmp::Ordering::Equal,
        _ => key(a).cmp(&key(b)),
    }
}

#[cfg(test)]
mod tests {
    use super::natural_hotkey_order;

    #[test]
    fn natural_order() {
        let mut v = vec!["Ctrl+10", "", "Ctrl+2", "Alt+1"];
        v.sort_by(|a, b| natural_hotkey_order(a, b));
        assert_eq!(v, vec!["Alt+1", "Ctrl+2", "Ctrl+10", ""]);
    }
}
