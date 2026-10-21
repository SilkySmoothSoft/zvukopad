//! Небольшие повторяющиеся элементы интерфейса.

use eframe::egui::{self, Color32, RichText};
use zv_audio::DeviceInfo;
use zv_core::i18n::tr;
use zv_core::settings::DeviceRef;
use zv_core::tf;

use super::{CaptureTarget, ZvApp, style};
use crate::hotkeys::{hotkey_from_egui, mouse_from_egui};

/// Длительность звука: короткие — с десятыми долями секунды.
pub fn fmt_duration(secs: f64) -> String {
    if secs < 10.0 { tf!("{} с", format!("{:.1}", secs)) } else { fmt_time(secs) }
}

pub fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Маленький индикатор уровня с подписью.
pub fn level_meter(ui: &mut egui::Ui, label: &str, level: f32, active: bool) {
    let _ = level_meter_resp(ui, label, level, active, true);
}

/// Индикатор уровня; возвращает область (подпись + полоска) для подсказок и кликов.
pub fn level_meter_resp(ui: &mut egui::Ui, label: &str, level: f32, active: bool, tooltip: bool) -> egui::Response {
    // В раскладке справа налево подпись должна идти после полоски.
    let rtl = ui.layout().prefer_right_to_left();
    let mut label_resp = None;
    if !rtl {
        label_resp = Some(
            ui.add(egui::Label::new(RichText::new(label).small().color(style::MUTED)).sense(egui::Sense::click())),
        );
    }
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(60.0, 10.0), egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 3.0, Color32::from_rgb(40, 42, 52));
    if active {
        // Логарифмическая шкала: −60 дБ … 0 дБ.
        let db = 20.0 * level.max(1e-6).log10();
        let frac = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        let color = if level > 0.95 {
            style::DANGER
        } else if level > 0.5 {
            style::ACCENT
        } else {
            style::PLAYING
        };
        let mut r = rect;
        r.set_width(rect.width() * frac);
        p.rect_filled(r, 3.0, color);
    }
    let resp = if tooltip {
        resp.on_hover_text(if active { label.to_string() } else { tf!("{}: выключено", label) })
    } else {
        resp
    };
    if rtl {
        label_resp = Some(
            ui.add(egui::Label::new(RichText::new(label).small().color(style::MUTED)).sense(egui::Sense::click())),
        );
    }
    match label_resp {
        Some(l) => resp.union(l),
        None => resp,
    }
}

/// Кнопка горячей клавиши: показывает текущую, по нажатию начинает назначение.
pub fn hotkey_button(ui: &mut egui::Ui, current: Option<&str>, error: Option<&String>) -> egui::Response {
    let text = match current {
        Some(k) if !k.is_empty() => RichText::new(k).monospace(),
        _ => RichText::new(tr("назначить")).italics().color(style::MUTED),
    };
    let text = if error.is_some() { text.color(style::DANGER) } else { text };
    let mut resp = ui.button(text);
    resp = match error {
        Some(e) => resp.on_hover_text(tf!("Не работает: {}", e)),
        None => resp.on_hover_text(tr("Нажмите, затем нажмите нужное сочетание клавиш")),
    };
    resp
}

/// Выпадающий список устройств. Возвращает `true`, если выбор поменялся.
pub fn device_combo(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut Option<DeviceRef>,
    devices: &[DeviceInfo],
    none_label: &str,
    mark_virtual: bool,
) -> bool {
    let selected_text = match value {
        None => none_label.to_string(),
        Some(r) => {
            if devices.iter().any(|d| d.id == r.id || d.name == r.name) {
                r.name.clone()
            } else {
                tf!("{} (не найдено)", r.name)
            }
        }
    };
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).width(360.0).selected_text(selected_text).show_ui(ui, |ui| {
        if ui.selectable_label(value.is_none(), none_label).clicked() {
            *value = None;
            changed = true;
        }
        for d in devices {
            let is_sel = value.as_ref().is_some_and(|r| r.id == d.id);
            let label = if mark_virtual && d.looks_virtual() { format!("★ {}", d.name) } else { d.name.clone() };
            if ui.selectable_label(is_sel, label).clicked() {
                *value = Some(d.to_ref());
                changed = true;
            }
        }
    });
    changed
}

/// Слайдер громкости в процентах.
pub fn volume_slider(ui: &mut egui::Ui, value: &mut f32, max: f32) -> egui::Response {
    ui.add(
        egui::Slider::new(value, 0.0..=max)
            .custom_formatter(|v, _| format!("{:.0} %", v * 100.0))
            .custom_parser(|s| s.trim().trim_end_matches('%').trim().parse::<f64>().ok().map(|v| v / 100.0)),
    )
}

/// Цвета для звуков.
pub const PALETTE: [[u8; 3]; 12] = [
    [235, 87, 87],
    [242, 153, 74],
    [242, 201, 76],
    [111, 207, 151],
    [39, 174, 96],
    [86, 204, 242],
    [47, 128, 237],
    [155, 81, 224],
    [187, 107, 217],
    [255, 128, 191],
    [160, 160, 170],
    [120, 92, 72],
];

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Выбор цвета: палитра + свой цвет + «без цвета». Возвращает новое значение, если выбрали.
pub fn color_picker(ui: &mut egui::Ui, current: Option<[u8; 3]>) -> Option<Option<[u8; 3]>> {
    let mut result = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for c in PALETTE {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
            ui.painter().rect_filled(rect, 4.0, rgb(c));
            if current == Some(c) {
                ui.painter().rect_stroke(
                    rect.expand(1.5),
                    5.0,
                    egui::Stroke::new(2.0, Color32::WHITE),
                    egui::StrokeKind::Outside,
                );
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                result = Some(Some(c));
            }
        }
        let mut custom = current.unwrap_or([200, 200, 200]);
        if ui.color_edit_button_srgb(&mut custom).on_hover_text(tr("Свой цвет")).changed() {
            result = Some(Some(custom));
        }
        if current.is_some() && ui.small_button("✖").on_hover_text(tr("Без цвета")).clicked() {
            result = Some(None);
        }
    });
    result
}

/// Окно «нажмите сочетание клавиш» (или кнопку мыши).
pub fn capture_dialog(app: &mut ZvApp, ctx: &egui::Context) {
    let Some(target) = app.capture.clone() else { return };

    // Модальное окно перехватывает клики — кнопки мыши ловим до показа, по сырым событиям.
    let mut result: Option<Option<String>> = None; // Some(None) — отмена
    let mut dialog_rect: Option<egui::Rect> = ctx.memory(|m| m.data.get_temp(egui::Id::unique("capture_rect")));
    ctx.input(|i| {
        for ev in &i.events {
            match ev {
                egui::Event::Key { key, pressed: true, modifiers, repeat: false, .. } => {
                    if *key == egui::Key::Escape {
                        result = Some(None);
                    } else if let Some(s) = hotkey_from_egui(*key, *modifiers) {
                        result = Some(Some(s));
                    }
                    break;
                }
                egui::Event::PointerButton { button, pressed: true, modifiers, pos } => {
                    // Клик левой кнопкой по кнопкам самого окна — не назначение.
                    let inside = dialog_rect.is_some_and(|r| r.contains(*pos));
                    if *button == egui::PointerButton::Primary && inside {
                        continue;
                    }
                    if let Some(s) = mouse_from_egui(*button, *modifiers) {
                        result = Some(Some(s));
                        break;
                    }
                }
                _ => {}
            }
        }
    });

    let title = match &target {
        CaptureTarget::Sound(id) => {
            tf!("Клавиша для «{}»", app.entry(id).map(|e| e.name.clone()).unwrap_or_default())
        }
        CaptureTarget::Action(a) => tf!("Клавиша: {}", a.label()),
        CaptureTarget::TagRandom(t) => tf!("Клавиша: случайный звук из #{}", t),
    };

    let resp = egui::Modal::new(egui::Id::unique("capture")).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.heading(title);
        ui.add_space(8.0);
        ui.label(tr("Нажмите сочетание клавиш (например Ctrl+Alt+1 или F13)"));
        ui.label(tr("или кнопку мыши: среднюю, боковые (Mouse4/Mouse5), левую/правую — с Ctrl/Alt/Shift."));
        ui.label(RichText::new(tr("Esc — отмена.")).color(style::MUTED));
        ui.add_space(6.0);
        ui.label(
            RichText::new(tr(
                "Совет: выбирайте сочетания с Ctrl/Alt — назначенная клавиша перестаёт работать в других программах \
                 (если не включён «пропуск нажатий в игру»).",
            ))
            .small()
            .color(style::MUTED),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(tr("Отмена")).clicked() {
                result = Some(None);
            }
            if ui.button(tr("Убрать клавишу")).clicked() {
                result = Some(Some(String::new()));
            }
        });
    });
    dialog_rect = Some(resp.response.rect);
    ctx.memory_mut(|m| {
        m.data.insert_temp(egui::Id::unique("capture_rect"), dialog_rect.unwrap_or(egui::Rect::NOTHING))
    });

    let Some(choice) = result else { return };
    app.capture = None;
    let Some(key) = choice else { return };
    let key = (!key.is_empty()).then_some(key);
    match target {
        CaptureTarget::Sound(id) => app.set_sound_hotkey(&id, key),
        CaptureTarget::Action(action) => {
            let new = key.unwrap_or_default();
            if !new.is_empty() && !new.eq_ignore_ascii_case(app.settings.controls.key(action)) {
                if let Some(owner) = app.meta.hotkey_owner(&new).map(str::to_string) {
                    let name = app.entry(&owner).map(|e| e.name.clone()).unwrap_or(owner.clone());
                    app.set_sound_hotkey(&owner, None);
                    app.toast(tf!("{} больше не у звука «{}»", new, name));
                }
                if let Some(other) = app.settings.controls.owner(&new)
                    && other != action
                {
                    app.settings.controls.set_key(other, String::new());
                    app.toast(tf!("{} больше не у действия «{}»", new, other.label()));
                }
                if let Some(tag) = app.settings.controls.tag_owner(&new).map(str::to_string) {
                    app.settings.controls.set_tag_key(&tag, String::new());
                    app.toast(tf!("{} больше не у «случайный из #{}»", new, tag));
                }
                if app.global_hotkey_owner(&new).is_some() && app.settings.controls.owner(&new).is_none() {
                    app.toast_error(tf!("{} занята набором номера на нумпаде", new));
                    return;
                }
            }
            app.settings.controls.set_key(action, new);
            app.mark_settings_dirty();
        }
        CaptureTarget::TagRandom(tag) => {
            let new = key.unwrap_or_default();
            if !new.is_empty() {
                if let Some(owner) = app.meta.hotkey_owner(&new).map(str::to_string) {
                    let name = app.entry(&owner).map(|e| e.name.clone()).unwrap_or(owner.clone());
                    app.set_sound_hotkey(&owner, None);
                    app.toast(tf!("{} больше не у звука «{}»", new, name));
                }
                if let Some(other) = app.settings.controls.tag_owner(&new).map(str::to_string)
                    && other != tag
                {
                    app.settings.controls.set_tag_key(&other, String::new());
                    app.toast(tf!("{} больше не у «случайный из #{}»", new, other));
                }
                if let Some(g) = app.settings.controls.owner(&new) {
                    app.toast_error(tf!("{} уже занята действием «{}»", new, g.label()));
                    return;
                }
                if app.global_hotkey_owner(&new).is_some() && app.settings.controls.tag_owner(&new).is_none() {
                    app.toast_error(tf!("{} занята набором номера на нумпаде", new));
                    return;
                }
            }
            app.settings.controls.set_tag_key(&tag, new);
            app.mark_settings_dirty();
        }
    }
}

/// Текстовое поле с меню правой кнопки: вырезать, копировать, вставить, копировать всё, очистить.
/// Ctrl+C / Ctrl+V / Ctrl+X / Ctrl+A работают и без меню.
pub fn text_edit_with_menu(
    ui: &mut egui::Ui,
    text: &mut String,
    multiline: bool,
    id: egui::Id,
    configure: impl FnOnce(egui::TextEdit<'_>) -> egui::TextEdit<'_>,
) -> egui::Response {
    let edit = if multiline { egui::TextEdit::multiline(text) } else { egui::TextEdit::singleline(text) };
    let output = configure(edit).id(id).show(ui);
    let resp = output.response.response.clone();
    let sel_key = id.with("zv_sel");
    let menu_key = id.with("zv_menu_sel");
    // Пока поле в фокусе — запоминаем выделение: при открытии меню фокус уходит и выделение сбрасывается.
    // Нажатие правой кнопки само переставляет курсор, поэтому в эти кадры выделение не обновляем.
    let right_btn =
        ui.input(|i| i.pointer.secondary_down() || i.pointer.secondary_pressed() || i.pointer.secondary_released());
    let focused = ui.ctx().memory(|m| m.has_focus(id));
    if focused
        && !right_btn
        && let Some(r) = output.state.cursor.char_range()
    {
        let (x, y) = (r.primary.index.0, r.secondary.index.0);
        ui.ctx().data_mut(|d| d.insert_temp(sel_key, (x.min(y), x.max(y))));
    }
    if resp.secondary_clicked() {
        let had_focus = focused;
        let sel = if had_focus { ui.ctx().data(|d| d.get_temp::<(usize, usize)>(sel_key)) } else { None };
        let end = text.chars().count();
        ui.ctx().data_mut(|d| d.insert_temp(menu_key, sel.unwrap_or((end, end))));
    }
    let (a, b) = ui.ctx().data(|d| d.get_temp::<(usize, usize)>(menu_key)).unwrap_or((0, 0));
    let mut action: Option<&'static str> = None;
    resp.context_menu(|ui| {
        let has_sel = a != b;
        if ui.add_enabled(has_sel, egui::Button::new(tr("✂ Вырезать")).shortcut_text("Ctrl+X")).clicked() {
            action = Some("cut");
        }
        if ui.add_enabled(has_sel, egui::Button::new(tr("📋 Копировать")).shortcut_text("Ctrl+C")).clicked()
        {
            action = Some("copy");
        }
        if ui.add(egui::Button::new(tr("📥 Вставить")).shortcut_text("Ctrl+V")).clicked() {
            action = Some("paste");
        }
        ui.separator();
        if ui.button(tr("Копировать весь текст")).clicked() {
            action = Some("copy_all");
        }
        if ui.button(tr("Очистить")).clicked() {
            action = Some("clear");
        }
        if action.is_some() {
            ui.close();
        }
    });
    let Some(act) = action else { return resp };
    let ctx = ui.ctx().clone();
    let byte = |t: &str, ci: usize| t.char_indices().nth(ci).map(|(i, _)| i).unwrap_or(t.len());
    let (ba, bb) = (byte(text, a), byte(text, b));
    let mut cursor: Option<usize> = None;
    match act {
        "copy" => ctx.copy_text(text[ba..bb].to_string()),
        "copy_all" => ctx.copy_text(text.clone()),
        "cut" => {
            ctx.copy_text(text[ba..bb].to_string());
            text.replace_range(ba..bb, "");
            cursor = Some(a);
        }
        "clear" => {
            text.clear();
            cursor = Some(0);
        }
        "paste" => match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(clip) => {
                let mut clip = clip.replace("\r\n", "\n");
                if !multiline {
                    clip = clip.replace('\n', " ");
                }
                text.replace_range(ba..bb, &clip);
                cursor = Some(a + clip.chars().count());
            }
            Err(e) => log::warn!("Буфер обмена: {e}"),
        },
        _ => {}
    }
    if let Some(c) = cursor {
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(c))));
        state.store(&ctx, id);
        ctx.data_mut(|d| d.insert_temp(sel_key, (c, c)));
        resp.request_focus();
    }
    resp
}

/// Значок глобуса — общепринятый знак выбора языка, понятен без слов.
pub fn globe_icon(painter: &egui::Painter, center: egui::Pos2, r: f32, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.3, color);
    painter.circle_stroke(center, r, stroke);
    // Меридиан посередине и «долька» вокруг него.
    painter.line_segment([center - egui::vec2(0.0, r), center + egui::vec2(0.0, r)], stroke);
    let n = 24;
    let pts: Vec<egui::Pos2> = (0..=n)
        .map(|i| {
            let t = std::f32::consts::PI * (i as f32 / n as f32 - 0.5);
            center + egui::vec2(r * 0.45 * t.cos(), r * t.sin())
        })
        .collect();
    painter.add(egui::Shape::line(pts.clone(), stroke));
    painter.add(egui::Shape::line(pts.iter().map(|p| egui::pos2(2.0 * center.x - p.x, p.y)).collect(), stroke));
    // Экватор и две параллели.
    painter.line_segment([center - egui::vec2(r, 0.0), center + egui::vec2(r, 0.0)], stroke);
    for dy in [-0.55f32, 0.55] {
        let y = center.y + r * dy;
        let half = r * (1.0 - dy * dy).sqrt();
        painter.line_segment([egui::pos2(center.x - half, y), egui::pos2(center.x + half, y)], stroke);
    }
}

/// Пункты выбора языка. `setting` — текущая настройка (`auto` или код). Возвращает новую настройку.
pub fn language_options(ui: &mut egui::Ui, setting: &str) -> Option<String> {
    let mut picked = None;
    let auto_code = zv_core::i18n::resolve_auto(&crate::sysutil::system_language());
    let langs = zv_core::i18n::available();
    let auto_name = langs.iter().find(|l| l.code == auto_code).map(|l| l.native_name.clone()).unwrap_or_default();
    let is_auto = setting.is_empty() || setting == "auto";
    if ui.selectable_label(is_auto, tf!("Как в системе ({})", auto_name)).clicked() {
        picked = Some("auto".to_string());
    }
    ui.separator();
    for l in &langs {
        let sel = !is_auto && l.code.eq_ignore_ascii_case(setting);
        // Название языка всегда на нём самом — его узнают, даже если сейчас выбран чужой язык.
        if ui.selectable_label(sel, format!("{}  ({})", l.native_name, l.code)).clicked() {
            picked = Some(l.code.clone());
        }
    }
    picked
}

/// Подпись «Язык · Language»: на текущем языке и всегда по-английски — чтобы найти, даже если язык непонятен.
pub fn language_caption() -> String {
    let local = tr("Язык");
    if local == "Language" { local.to_string() } else { format!("{local} · Language") }
}

/// Кнопка выбора языка для верхней панели: глобус + код языка.
pub fn language_button(ui: &mut egui::Ui, setting: &str) -> Option<String> {
    let code = zv_core::i18n::current_code().to_uppercase();
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(code, font, ui.visuals().text_color());
    let size = egui::vec2(galley.size().x + 34.0, ui.spacing().interact_size.y.max(22.0));
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let visuals = ui.style().interact(&resp);
    ui.painter().rect_filled(rect, 4.0, visuals.weak_bg_fill);
    let r = rect.height() * 0.32;
    let gc = egui::pos2(rect.left() + 6.0 + r, rect.center().y);
    globe_icon(ui.painter(), gc, r, visuals.text_color());
    ui.painter().galley(
        egui::pos2(gc.x + r + 6.0, rect.center().y - galley.size().y / 2.0),
        galley,
        visuals.text_color(),
    );
    let resp = resp.on_hover_text(language_caption());
    let mut picked = None;
    egui::Popup::menu(&resp).show(|ui| {
        ui.label(RichText::new(language_caption()).small().color(style::MUTED));
        if let Some(p) = language_options(ui, setting) {
            picked = Some(p);
            ui.close();
        }
    });
    picked
}

/// Путь к папке с кнопкой «скрыть/показать». Скрытие действует сразу везде в программе
/// (Настройки → Папки → «Скрывать пути»): в путях есть имя пользователя Windows.
pub fn path_value(app: &mut ZvApp, ui: &mut egui::Ui, path: &std::path::Path) {
    if app.paths_hidden() {
        if ui
            .add(egui::Button::new(RichText::new(tr("🔒 путь скрыт")).small()).small())
            .on_hover_text(tr("Показать пути к папкам (везде в программе)"))
            .clicked()
        {
            app.set_paths_hidden(false);
        }
    } else {
        // Кнопка — перед путём: длинный путь обрезается справа, а кнопка всегда видна.
        if ui
            .add(egui::Button::new(RichText::new(tr("🙈 скрыть")).small()).small())
            .on_hover_text(tr("Скрыть пути к папкам (везде в программе) — для стримов и скриншотов"))
            .clicked()
        {
            app.set_paths_hidden(true);
        }
        let text = path.display().to_string();
        ui.add(egui::Label::new(RichText::new(&text).small().color(style::MUTED)).truncate()).on_hover_text(&text);
    }
}
