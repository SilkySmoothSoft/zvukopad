//! Вкладка «Звуки»: библиотека, проигрывание, горячие клавиши.

use eframe::egui::{self, RichText};
use zv_core::i18n::tr;
use zv_core::settings::PlayTarget;
use zv_core::tf;

use super::{CaptureTarget, ZvApp, style, widgets};
use crate::sysutil;

pub fn show(app: &mut ZvApp, ui: &mut egui::Ui) {
    if app.settings.ui.tags_panel_open {
        egui::Panel::left("tags").resizable(true).default_size(170.0).min_size(130.0).show(ui, |ui| {
            tags_panel(app, ui);
        });
    }
    if app.settings.ui.queue_panel_open {
        egui::Panel::bottom("queue").resizable(true).default_size(180.0).min_size(110.0).show(ui, |ui| {
            queue_panel(app, ui);
        });
    }
    if app.selected.is_some() {
        egui::Panel::right("details").resizable(true).default_size(300.0).min_size(260.0).show(ui, |ui| {
            details(app, ui);
        });
    }
    egui::CentralPanel::default().show(ui, |ui| {
        toolbar(app, ui);
        ui.separator();
        list(app, ui);
    });
}

fn toolbar(app: &mut ZvApp, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        let mut tags_open = app.settings.ui.tags_panel_open;
        if ui.toggle_value(&mut tags_open, tr("🏷 Теги")).on_hover_text(tr("Панель тегов")).changed() {
            app.settings.ui.tags_panel_open = tags_open;
            app.mark_settings_dirty();
        }
        let qlen = app.queue.len();
        let mut q_open = app.settings.ui.queue_panel_open;
        let q_label =
            if qlen > 0 { tf!("📋 Очередь ({})", qlen) } else { tr("📋 Очередь").to_string() };
        if ui.toggle_value(&mut q_open, q_label).on_hover_text(tr("Панель очереди")).changed() {
            app.settings.ui.queue_panel_open = q_open;
            app.mark_settings_dirty();
        }
        let search =
            ui.add(egui::TextEdit::singleline(&mut app.search).hint_text(tr("🔍 Поиск (Ctrl+F)")).desired_width(200.0));
        if app.focus_search {
            search.request_focus();
            app.focus_search = false;
        }
        if !app.search.is_empty() && ui.small_button("✖").clicked() {
            app.search.clear();
        }
        let cats = app.categories();
        let label = match &app.category {
            None => tr("Все папки").to_string(),
            Some(c) if c.is_empty() => tr("Без папки").to_string(),
            Some(c) => c.clone(),
        };
        egui::ComboBox::from_id_salt("cat").selected_text(label).width(160.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut app.category, None, tr("Все папки"));
            for c in cats {
                let text = if c.is_empty() { tr("Без папки").to_string() } else { c.clone() };
                ui.selectable_value(&mut app.category, Some(c), text);
            }
        });
        if ui.button(tr("📂 Папка звуков")).on_hover_text(app.mask(&app.sounds_dir.display().to_string())).clicked()
        {
            sysutil::open_folder(&app.sounds_dir);
        }
        if ui.button(tr("⟳ Обновить")).on_hover_text(tr("Перечитать папку звуков")).clicked()
        {
            app.rescan();
        }
    });
    ui.label(
        RichText::new(tr("Чтобы добавить звуки, перетащите файлы в окно (wav, mp3, ogg, flac, m4a)."))
            .small()
            .color(style::MUTED),
    );
}

fn list(app: &mut ZvApp, ui: &mut egui::Ui) {
    // Клавиша Delete удаляет выбранный звук (если не идёт ввод текста).
    if let Some(sel) = app.selected.clone()
        && app.capture.is_none()
        && app.editor.is_none()
        && app.inline_rename.is_none()
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::Delete))
    {
        app.pending_delete = Some(sel);
    }
    if let Some(sel) = app.selected.clone()
        && app.capture.is_none()
        && app.inline_rename.is_none()
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::F2))
    {
        app.start_inline_rename(&sel);
    }
    if let Some(sel) = app.selected.clone()
        && app.settings.controls.enter_plays
        && app.capture.is_none()
        && app.inline_rename.is_none()
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::Enter))
    {
        app.player.play(&sel, None);
    }
    if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
        app.focus_search = true;
    }
    let visible = visible_entries(app);

    if app.entries.is_empty() {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(tr("Здесь пока пусто")).size(20.0));
            ui.add_space(6.0);
            ui.label(tr("Перетащите звуковые файлы в это окно,"));
            ui.label(tr("запишите звук на вкладке «Запись» или создайте речь на вкладке «Синтез речи»."));
            ui.add_space(10.0);
            if ui.button(tr("📂 Открыть папку звуков")).clicked() {
                sysutil::open_folder(&app.sounds_dir);
            }
        });
        return;
    }
    if visible.is_empty() {
        ui.label(RichText::new(tr("Ничего не найдено")).color(style::MUTED));
        return;
    }

    let row_h = 30.0;
    egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, row_h, visible.len(), |ui, range| {
        for &idx in &visible[range] {
            let e = app.entries[idx].clone();
            let meta = app.meta.get(&e.id);
            let playing = app.is_playing(&e.id);
            let selected = app.selected.as_deref() == Some(e.id.as_str());
            ui.horizontal(|ui| {
                ui.set_height(row_h - 4.0);
                // Цветная метка звука.
                let (strip, _) = ui.allocate_exact_size(egui::vec2(4.0, 22.0), egui::Sense::hover());
                if let Some(c) = meta.color {
                    ui.painter().rect_filled(strip, 2.0, widgets::rgb(c));
                }
                let play = egui::Button::new(if playing { "■" } else { "▶" }).min_size(egui::vec2(28.0, 24.0));
                let hint = format!(
                    "{} — {}",
                    if playing { tr("Остановить") } else { tr("Играть") },
                    app.settings.audio.target.label()
                );
                if ui.add(play).on_hover_text(hint).clicked() {
                    if playing {
                        app.player.stop(&e.id);
                    } else {
                        app.player.play(&e.id, None);
                    }
                }
                // Пока звук играет, вторая кнопка — пауза именно этого звука.
                let pos = if playing { app.player.position(&e.id) } else { None };
                if let Some((_, paused)) = pos {
                    let (icon, hint) = if paused {
                        ("▶", tr("Продолжить этот звук"))
                    } else {
                        ("⏸", tr("Пауза этого звука"))
                    };
                    if ui.add(egui::Button::new(icon).min_size(egui::vec2(28.0, 24.0))).on_hover_text(hint).clicked() {
                        app.player.toggle_sound_pause(&e.id);
                    }
                } else if ui
                    .add(egui::Button::new("🎧").min_size(egui::vec2(28.0, 24.0)))
                    .on_hover_text(tr("Прослушать только себе"))
                    .clicked()
                {
                    app.player.play(&e.id, Some(PlayTarget::Monitor));
                }
                let dur = match app.player.duration(&e.id) {
                    Some(d) => widgets::fmt_duration(d),
                    None if app.player.is_loading(&e.id) => "…".into(),
                    None => String::new(),
                };
                let mut right = dur;
                if !e.category.is_empty() && app.category.is_none() {
                    right = format!("{}   {right}", e.category);
                }
                if !meta.tags.is_empty() {
                    right = format!(
                        "{}   {right}",
                        meta.tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" ")
                    );
                }
                if let (Some(code), true) = (meta.code, app.settings.numpad.enabled) {
                    right = format!("№{code}   {right}");
                }
                let width = (ui.available_width() - 270.0).max(160.0);
                let (resp, renaming) = name_cell(app, ui, &e, width, selected, playing, &right);
                // Второй клик двойного щелчка не снимает выделение.
                if resp.clicked() && !resp.double_clicked() {
                    app.select(if selected { None } else { Some(e.id.clone()) });
                }
                if resp.double_clicked() && !renaming {
                    app.player.play(&e.id, None);
                }
                // Правый клик по строке — меню действий.
                resp.context_menu(|ui| {
                    if ui.button(tr("▶ Играть")).clicked() {
                        app.player.play(&e.id, None);
                        ui.close();
                    }
                    if ui.button(tr("🎧 Прослушать себе")).clicked() {
                        app.player.play(&e.id, Some(PlayTarget::Monitor));
                        ui.close();
                    }
                    if ui.button(tr("➕ В очередь")).clicked() {
                        app.queue.add(&e.id);
                        ui.close();
                    }
                    if ui.button(tr("⌨ Назначить клавишу")).clicked() {
                        app.capture = Some(CaptureTarget::Sound(e.id.clone()));
                        ui.close();
                    }
                    ui.menu_button(tr("🎨 Цвет"), |ui| {
                        if let Some(c) = widgets::color_picker(ui, meta.color) {
                            app.meta.update(&e.id, |m| m.color = c);
                            app.mark_meta_dirty();
                            ui.close();
                        }
                    });
                    let all_tags = app.meta.all_tags();
                    if !all_tags.is_empty() {
                        ui.menu_button(tr("🏷 Теги"), |ui| {
                            for (t, _) in all_tags {
                                let mut has = meta.tags.contains(&t);
                                if ui.checkbox(&mut has, &t).changed() {
                                    toggle_tag(app, &e.id, &t, has);
                                }
                            }
                        });
                    }
                    if ui.button(tr("✏ Переименовать  (F2)")).clicked() {
                        app.start_inline_rename(&e.id);
                        ui.close();
                    }
                    if ui.button(tr("✂ Обрезать…")).clicked() {
                        app.open_editor(&e.id);
                        ui.close();
                    }
                    if ui.button(tr("📂 Показать в папке")).clicked() {
                        sysutil::show_in_folder(&e.path);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(RichText::new(tr("🗑 Удалить  (Del)")).color(style::DANGER)).clicked() {
                        app.pending_delete = Some(e.id.clone());
                        ui.close();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("🗑").color(style::DANGER)).min_size(egui::vec2(28.0, 24.0)),
                        )
                        .on_hover_text(tr("Удалить (в Корзину)"))
                        .clicked()
                    {
                        app.pending_delete = Some(e.id.clone());
                    }
                    let err = meta.hotkey.as_ref().and_then(|h| app.hotkeys.errors.get(h));
                    if widgets::hotkey_button(ui, meta.hotkey.as_deref(), err).clicked() {
                        app.capture = Some(CaptureTarget::Sound(e.id.clone()));
                    }
                });
            });
        }
    });
}

/// Индексы звуков, которые проходят фильтры (папка, поиск, теги).
pub fn visible_entries(app: &ZvApp) -> Vec<usize> {
    let needle = app.search.to_lowercase();
    let match_all = app.settings.ui.tags_match_all;
    app.entries
        .iter()
        .enumerate()
        .filter(|(_, e)| app.category.as_ref().is_none_or(|c| &e.category == c))
        .filter(|(_, e)| {
            if app.selected_tags.is_empty() {
                return true;
            }
            let tags = app.meta.sounds.get(&e.id).map(|m| m.tags.as_slice()).unwrap_or(&[]);
            if match_all {
                app.selected_tags.iter().all(|t| tags.contains(t))
            } else {
                app.selected_tags.iter().any(|t| tags.contains(t))
            }
        })
        .filter(|(_, e)| {
            if needle.is_empty() {
                return true;
            }
            let m = app.meta.sounds.get(&e.id);
            e.name.to_lowercase().contains(&needle)
                || e.category.to_lowercase().contains(&needle)
                || m.is_some_and(|m| m.tags.iter().any(|t| t.to_lowercase().contains(&needle)))
                || m.and_then(|m| m.code).is_some_and(|c| c.to_string() == needle)
        })
        .map(|(i, _)| i)
        .collect()
}

fn toggle_tag(app: &mut ZvApp, id: &str, tag: &str, on: bool) {
    app.meta.update(id, |m| {
        m.tags.retain(|t| t != tag);
        if on {
            m.tags.push(tag.to_string());
            m.tags.sort_by_key(|t| t.to_lowercase());
        }
    });
    app.mark_meta_dirty();
}

/// Панель тегов: фильтр, переименование, удаление.
fn tags_panel(app: &mut ZvApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("🏷 Теги")).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("◀").on_hover_text(tr("Свернуть")).clicked() {
                app.settings.ui.tags_panel_open = false;
                app.mark_settings_dirty();
            }
        });
    });
    let tags = app.meta.all_tags();
    if tags.is_empty() {
        ui.label(
            RichText::new(tr(
                "Тегов пока нет. Добавьте их в карточке звука (клик по строке) — например «мемы», «музыка».",
            ))
            .small()
            .color(style::MUTED),
        );
        return;
    }
    ui.horizontal(|ui| {
        let mut all = app.settings.ui.tags_match_all;
        if ui.selectable_label(!all, tr("любой")).on_hover_text(tr("Звуки с любым из выбранных тегов")).clicked()
        {
            all = false;
        }
        if ui.selectable_label(all, tr("все")).on_hover_text(tr("Звуки со всеми выбранными тегами")).clicked()
        {
            all = true;
        }
        if all != app.settings.ui.tags_match_all {
            app.settings.ui.tags_match_all = all;
            app.mark_settings_dirty();
        }
        if !app.selected_tags.is_empty() && ui.small_button(tr("✖ сброс")).clicked() {
            app.selected_tags.clear();
        }
    });
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (tag, count) in tags {
            // Переименование прямо в списке.
            if let Some((old, buf)) = app.tag_rename.as_mut()
                && *old == tag
            {
                let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(f32::INFINITY));
                resp.request_focus();
                if resp.lost_focus() {
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let (old, new) = app.tag_rename.take().unwrap_or_default();
                        let new = zv_core::library::normalize_tag(&new);
                        if !new.is_empty() && new != old {
                            app.meta.rename_tag(&old, &new);
                            if let Some(k) = app.settings.controls.tag_random.remove(&old) {
                                app.settings.controls.tag_random.insert(new.clone(), k);
                                app.mark_settings_dirty();
                            }
                            if app.selected_tags.remove(&old) {
                                app.selected_tags.insert(new);
                            }
                            app.mark_meta_dirty();
                        }
                    } else {
                        app.tag_rename = None;
                    }
                }
                continue;
            }
            let on = app.selected_tags.contains(&tag);
            let rkey = app.settings.controls.tag_random.get(&tag).cloned();
            let text = match &rkey {
                Some(k) => format!("#{tag}  ({count})  🎲 {k}"),
                None => format!("#{tag}  ({count})"),
            };
            let resp = ui.selectable_label(on, text);
            if resp.clicked() {
                if on {
                    app.selected_tags.remove(&tag);
                } else {
                    app.selected_tags.insert(tag.clone());
                }
            }
            resp.context_menu(|ui| {
                if ui.button(tr("🎲 Сыграть случайный из тега")).clicked() {
                    app.play_random_from_tag(&tag);
                    ui.close();
                }
                let key_label = match &rkey {
                    Some(k) => tf!("⌨ Клавиша «случайный»: {}…", k),
                    None => tr("⌨ Назначить клавишу «случайный из тега»…").to_string(),
                };
                if ui.button(key_label).clicked() {
                    app.capture = Some(CaptureTarget::TagRandom(tag.clone()));
                    ui.close();
                }
                if rkey.is_some() && ui.button(tr("✖ Убрать клавишу «случайный»")).clicked() {
                    app.settings.controls.set_tag_key(&tag, String::new());
                    app.mark_settings_dirty();
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("✏ Переименовать")).clicked() {
                    app.tag_rename = Some((tag.clone(), tag.clone()));
                    ui.close();
                }
                if ui.button(tr("➕ Все звуки с тегом — в очередь")).clicked() {
                    let ids: Vec<String> = app
                        .entries
                        .iter()
                        .filter(|e| app.meta.sounds.get(&e.id).is_some_and(|m| m.tags.contains(&tag)))
                        .map(|e| e.id.clone())
                        .collect();
                    for id in ids {
                        app.queue.add(&id);
                    }
                    ui.close();
                }
                ui.separator();
                if ui
                    .button(RichText::new(tr("🗑 Удалить тег")).color(style::DANGER))
                    .on_hover_text(tr("Тег уберётся у всех звуков, сами звуки останутся"))
                    .clicked()
                {
                    app.meta.remove_tag(&tag);
                    if app.settings.controls.tag_random.remove(&tag).is_some() {
                        app.mark_settings_dirty();
                    }
                    app.selected_tags.remove(&tag);
                    app.mark_meta_dirty();
                    app.toast(tf!("Тег «{}» удалён (звуки на месте)", tag));
                    ui.close();
                }
            });
        }
    });
}

/// Панель очереди.
fn queue_panel(app: &mut ZvApp, ui: &mut egui::Ui) {
    let q = app.queue.snapshot();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("📋 Очередь")).strong());
        if ui
            .button(if q.playing { tr("■ Стоп") } else { tr("▶ Играть") })
            .on_hover_text(app.settings.controls.key(zv_core::controls::Action::QueuePlayStop).to_string())
            .clicked()
        {
            app.queue.toggle();
        }
        if ui.button("⏭").on_hover_text(tr("Следующий")).clicked() {
            app.queue.next();
        }
        let (mut repeat, mut shuffle, mut gap) = (q.repeat, q.shuffle, q.gap);
        ui.toggle_value(&mut repeat, "🔁").on_hover_text(tr("По кругу"));
        ui.toggle_value(&mut shuffle, "🔀").on_hover_text(tr("Вперемешку"));
        ui.label(tr("пауза:"));
        ui.add(egui::DragValue::new(&mut gap).range(0.0..=60.0).speed(0.1).suffix(tr(" с")).fixed_decimals(1));
        if (repeat, shuffle, gap) != (q.repeat, q.shuffle, q.gap) {
            app.queue.set_options(repeat, shuffle, gap);
        }
        if ui.button(tr("➕ Видимые")).on_hover_text(tr("Добавить все звуки, которые сейчас в списке")).clicked()
        {
            let ids: Vec<String> = visible_entries(app).into_iter().map(|i| app.entries[i].id.clone()).collect();
            for id in ids {
                app.queue.add(&id);
            }
        }
        if !q.items.is_empty() && ui.button(tr("Очистить")).clicked() {
            app.queue.clear();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("▼").on_hover_text(tr("Свернуть")).clicked() {
                app.settings.ui.queue_panel_open = false;
                app.mark_settings_dirty();
            }
        });
    });
    ui.separator();
    if q.items.is_empty() {
        ui.label(
            RichText::new(tr(
                "Пусто. Добавляйте звуки: правый клик → «В очередь», или кнопка «В очередь» в карточке звука.",
            ))
            .color(style::MUTED),
        );
        return;
    }
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (i, id) in q.items.iter().enumerate() {
            let name = app.entry(id).map(|e| e.name.clone()).unwrap_or_else(|| tf!("{} (нет файла)", id));
            let current = q.current == Some(i);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{:>3}.", i + 1)).monospace().color(style::MUTED));
                let mut text = RichText::new(&name);
                if current && q.playing {
                    text = text.color(style::PLAYING).strong();
                } else if current {
                    text = text.color(style::ACCENT);
                }
                if ui.selectable_label(current, text).on_hover_text(tr("Двойной клик — играть отсюда")).double_clicked()
                {
                    app.queue.play_from(i);
                }
                if ui.small_button("↑").clicked() {
                    app.queue.move_item(i, true);
                }
                if ui.small_button("↓").clicked() {
                    app.queue.move_item(i, false);
                }
                if ui.small_button("✖").clicked() {
                    app.queue.remove(i);
                }
            });
        }
    });
}

/// Ячейка с названием звука.
///
/// * Клик по самому названию (курсор — текстовая каретка) — переименовать прямо в списке:
///   Enter — сохранить, Esc — отмена.
/// * Клик по остальной части строки — выбрать звук (справа откроется карточка).
///
/// Длинные названия обрезаются примерно до половины строки и заканчиваются «…».
fn name_cell(
    app: &mut ZvApp,
    ui: &mut egui::Ui,
    e: &zv_core::library::SoundEntry,
    width: f32,
    selected: bool,
    playing: bool,
    right: &str,
) -> (egui::Response, bool) {
    let height = 24.0;
    let (rect, row) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let visuals = ui.visuals().clone();
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 4.0, visuals.selection.bg_fill);
    } else if row.hovered() {
        painter.rect_filled(rect, 4.0, visuals.widgets.hovered.weak_bg_fill);
    }
    // Тонкая полоска прогресса внизу строки, пока звук играет.
    if playing
        && let (Some((secs, paused)), Some(dur)) = (app.player.position(&e.id), app.player.duration(&e.id))
        && dur > 0.0
    {
        let frac = (secs / dur).clamp(0.0, 1.0) as f32;
        let bar = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 4.0, rect.bottom() - 3.0),
            egui::vec2((rect.width() - 8.0) * frac, 2.0),
        );
        painter.rect_filled(bar, 1.0, if paused { style::ACCENT } else { style::PLAYING });
    }
    // Справа — папка и длительность.
    let right_font = egui::TextStyle::Small.resolve(ui.style());
    painter.text(
        egui::pos2(rect.right() - 8.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        right,
        right_font,
        style::MUTED,
    );

    let name_max = (rect.width() * 0.55).max(120.0);
    let name_rect = egui::Rect::from_min_size(rect.min + egui::vec2(8.0, 0.0), egui::vec2(name_max, height));

    let editing = app.inline_rename.as_ref().is_some_and(|(id, _, _)| id == &e.id);
    if editing {
        let mut commit = false;
        let mut cancel = false;
        let edit_rect =
            egui::Rect::from_min_size(name_rect.min + egui::vec2(-4.0, 1.0), egui::vec2(name_max, height - 2.0));
        let buttons_rect =
            egui::Rect::from_min_size(egui::pos2(edit_rect.right() + 4.0, rect.top()), egui::vec2(60.0, height));
        if let Some((_, buf, focus)) = app.inline_rename.as_mut() {
            let te = egui::TextEdit::singleline(buf).desired_width(name_max - 8.0).char_limit(80);
            let resp = ui.put(edit_rect, te);
            if *focus {
                resp.request_focus();
                *focus = false;
            }
            if resp.lost_focus() {
                // Enter — сохранить; Esc или клик мимо — отмена (кнопка ✔ сработает ниже).
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    commit = true;
                } else {
                    cancel = true;
                }
            }
        }
        ui.scope_builder(egui::UiBuilder::new().max_rect(buttons_rect), |ui| {
            ui.horizontal_centered(|ui| {
                if ui.small_button("✔").on_hover_text(tr("Сохранить (Enter)")).clicked() {
                    commit = true;
                }
                if ui.small_button("✖").on_hover_text(tr("Отмена (Esc)")).clicked() {
                    cancel = true;
                }
            });
        });
        if commit {
            if let Some((id, buf, _)) = app.inline_rename.take() {
                let new_name = buf.trim().to_string();
                if !new_name.is_empty() && new_name != e.name {
                    rename_to(app, &id, &e.path, &new_name);
                }
            }
        } else if cancel {
            app.inline_rename = None;
        }
        return (row, false);
    }

    let mut text = RichText::new(&e.name);
    if playing {
        text = text.color(style::PLAYING).strong();
    } else if selected {
        text = text.color(visuals.selection.stroke.color);
    }
    // Название само клики не ловит: двойной клик по нему — переименование,
    // правый — то же меню, что и у всей строки (оно привязано к строке).
    let label = egui::Label::new(text).truncate().selectable(false);
    let name_resp = ui
        .scope_builder(
            egui::UiBuilder::new().max_rect(name_rect).layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| ui.add(label),
        )
        .inner;
    let over_name = |p: Option<egui::Pos2>| p.is_some_and(|p| name_resp.rect.contains(p));
    let row = if over_name(row.hover_pos()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        row.on_hover_text(tf!("{}\nДвойной клик — переименовать (или F2), правый клик — меню", e.name))
    } else {
        row
    };
    if row.double_clicked() && over_name(row.interact_pointer_pos()) {
        app.start_inline_rename(&e.id);
        return (row, true);
    }
    (row, false)
}

fn details(app: &mut ZvApp, ui: &mut egui::Ui) {
    let Some(id) = app.selected.clone() else { return };
    let Some(entry) = app.entry(&id).cloned() else {
        app.selected = None;
        return;
    };
    let meta = app.meta.get(&id);

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading(RichText::new(&entry.name).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("✖").on_hover_text(tr("Закрыть")).clicked() {
                app.selected = None;
            }
        });
    });
    if !entry.category.is_empty() {
        ui.label(RichText::new(tf!("Папка: {}", entry.category)).color(style::MUTED));
    }
    if let Some(d) = app.player.duration(&id) {
        ui.label(RichText::new(tf!("Длительность: {}", widgets::fmt_duration(d))).color(style::MUTED));
    }
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        let playing = app.is_playing(&id);
        if ui.button(if playing { tr("■ Стоп") } else { tr("▶ Играть") }).clicked() {
            if playing {
                app.player.stop(&id);
            } else {
                app.player.play(&id, None);
            }
        }
        if let Some((_, paused)) = app.player.position(&id) {
            let label = if paused { tr("▶ Продолжить") } else { tr("⏸ Пауза") };
            if ui.button(label).on_hover_text(tr("Пауза только этого звука")).clicked() {
                app.player.toggle_sound_pause(&id);
            }
        }
        if ui.button(tr("🎧 Себе")).clicked() {
            app.player.play(&id, Some(PlayTarget::Monitor));
        }
        if ui.button(tr("🎤 В микрофон")).clicked() {
            app.player.play(&id, Some(PlayTarget::Virtual));
        }
        if ui.button(tr("➕ В очередь")).clicked() {
            app.queue.add(&id);
        }
    });
    sound_waveform(app, ui, &id);
    ui.separator();

    ui.label(RichText::new(tr("Горячая клавиша")).strong());
    ui.horizontal(|ui| {
        let err = meta.hotkey.as_ref().and_then(|h| app.hotkeys.errors.get(h));
        if widgets::hotkey_button(ui, meta.hotkey.as_deref(), err).clicked() {
            app.capture = Some(CaptureTarget::Sound(id.clone()));
        }
        if meta.hotkey.is_some() && ui.small_button(tr("Убрать")).clicked() {
            app.set_sound_hotkey(&id, None);
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.manual_hotkey_buf)
                .hint_text(tr("или вручную: F13, Mouse4"))
                .desired_width(170.0),
        );
        if ui.button("OK").on_hover_text(tr("Для клавиш, которые не ловятся нажатием (Numpad, F13–F24)")).clicked()
        {
            let text = app.manual_hotkey_buf.trim().to_string();
            if text.is_empty() {
                app.set_sound_hotkey(&id, None);
            } else {
                match crate::keys::validate(&text) {
                    Ok(()) => app.set_sound_hotkey(&id, Some(text)),
                    Err(e) => app.toast_error(e),
                }
            }
        }
    });
    ui.separator();

    if app.settings.numpad.enabled {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Номер на нумпаде")).strong());
            // Номер применяется, только когда ввод закончен (Enter или клик в другое место),
            // а не на каждой цифре — иначе при наборе «11» сразу отнимется номер «1» у другого звука.
            let editing = app.code_edit.as_ref().is_some_and(|(eid, _)| eid == &id);
            let mut buf = if editing {
                app.code_edit.as_ref().map(|(_, b)| b.clone()).unwrap_or_default()
            } else {
                meta.code.map(|c| c.to_string()).unwrap_or_default()
            };
            let resp = ui.add(
                egui::TextEdit::singleline(&mut buf)
                    .id_salt(("numpad_code", id.as_str()))
                    .desired_width(80.0)
                    .hint_text(tr("нет"))
                    .char_limit(6),
            );
            buf.retain(|c| c.is_ascii_digit());
            if resp.has_focus() || resp.changed() {
                app.code_edit = Some((id.clone(), buf));
            }
            if resp.lost_focus() {
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    app.code_edit = None;
                } else {
                    app.commit_code_edit();
                }
            }
            resp.on_hover_text(tf!(
                "Зажмите {}, наберите этот номер на нумпаде и отпустите. Пусто — без номера. Применяется после Enter или клика мимо поля.",
                app.settings.numpad.modifier.label()
            ));
        });
        ui.separator();
    }

    ui.label(RichText::new(tr("Цвет")).strong());
    if let Some(c) = widgets::color_picker(ui, meta.color) {
        app.meta.update(&id, |m| m.color = c);
        app.mark_meta_dirty();
    }
    ui.separator();

    ui.label(RichText::new(tr("Теги")).strong());
    ui.horizontal_wrapped(|ui| {
        for t in meta.tags.clone() {
            if ui.small_button(format!("#{t} ✖")).on_hover_text(tr("Убрать тег у этого звука")).clicked()
            {
                toggle_tag(app, &id, &t, false);
            }
        }
    });
    ui.horizontal(|ui| {
        let resp =
            ui.add(egui::TextEdit::singleline(&mut app.tag_input).hint_text(tr("новый тег")).desired_width(140.0));
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button(tr("Добавить")).clicked() || enter) && !app.tag_input.trim().is_empty() {
            let t = zv_core::library::normalize_tag(&app.tag_input);
            if !t.is_empty() {
                toggle_tag(app, &id, &t, true);
            }
            app.tag_input.clear();
        }
    });
    // Быстрое добавление уже существующих тегов.
    let suggestions: Vec<String> =
        app.meta.all_tags().into_iter().map(|(t, _)| t).filter(|t| !meta.tags.contains(t)).take(12).collect();
    if !suggestions.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for t in suggestions {
                if ui.small_button(format!("+ {t}")).clicked() {
                    toggle_tag(app, &id, &t, true);
                }
            }
        });
    }
    let meta = app.meta.get(&id);
    let mut meta = meta;
    ui.separator();

    ui.label(RichText::new(tr("Громкость звука")).strong());
    let mut vol = meta.volume;
    if widgets::volume_slider(ui, &mut vol, 2.0).changed() {
        meta.volume = vol;
        app.meta.set(&id, meta.clone());
        app.player.update_slot(&id, vol, meta.hotkey.is_some());
        app.mark_meta_dirty();
    }
    ui.separator();

    ui.label(RichText::new(tr("Название")).strong());
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut app.rename_buf).desired_width(180.0));
        if ui.button(tr("Переименовать")).clicked() {
            rename(app, &id, &entry.path);
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button(tr("✂ Обрезать")).clicked() {
            app.open_editor(&id);
        }
        if ui.button(tr("📂 Показать в папке")).clicked() {
            sysutil::show_in_folder(&entry.path);
        }
        if ui.button(RichText::new(tr("🗑 Удалить")).color(style::DANGER)).clicked() {
            app.pending_delete = Some(id.clone());
        }
    });
}

fn rename(app: &mut ZvApp, id: &str, path: &std::path::Path) {
    let new_name = app.rename_buf.trim().to_string();
    if !new_name.is_empty() {
        rename_to(app, id, path, &new_name);
    }
}

pub fn rename_to(app: &mut ZvApp, id: &str, path: &std::path::Path, new_name: &str) {
    app.player.stop(id);
    match zv_core::library::rename_file(path, new_name) {
        Ok(new_path) => {
            if let Some(new_id) = zv_core::library::id_for(&app.sounds_dir, &new_path) {
                app.meta.rename(id, &new_id);
                app.player.forget(id);
                app.mark_meta_dirty();
                app.rescan();
                app.select(Some(new_id));
            }
        }
        Err(e) => app.toast_error(tf!("Не удалось переименовать: {}", e)),
    }
}

/// Подтверждение удаления.
pub fn delete_dialog(app: &mut ZvApp, ctx: &egui::Context) {
    let Some(id) = app.pending_delete.clone() else { return };
    let Some(entry) = app.entry(&id).cloned() else {
        app.pending_delete = None;
        return;
    };
    let mut decision: Option<bool> = None;
    egui::Modal::new(egui::Id::unique("delete")).show(ctx, |ui| {
        ui.set_width(360.0);
        ui.heading(tr("Удалить звук?"));
        ui.add_space(6.0);
        ui.label(tf!("«{}» будет перемещён в Корзину.", entry.name));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new(tr("Удалить")).color(style::DANGER)).clicked() {
                decision = Some(true);
            }
            if ui.button(tr("Отмена")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                decision = Some(false);
            }
        });
    });
    match decision {
        Some(true) => {
            app.pending_delete = None;
            app.player.stop(&id);
            match sysutil::delete_to_recycle_bin(&entry.path) {
                Ok(()) => {
                    app.meta.set(&id, Default::default());
                    app.player.forget(&id);
                    app.mark_meta_dirty();
                    app.selected = None;
                    app.rescan();
                    app.toast(tf!("«{}» удалён в Корзину", entry.name));
                }
                Err(e) => app.toast_error(e),
            }
        }
        Some(false) => app.pending_delete = None,
        None => {}
    }
}

/// Волна звука с бегунком. Клик по волне — перемотка (если звук играет) или запуск с этого места.
fn sound_waveform(app: &mut ZvApp, ui: &mut egui::Ui, id: &str) {
    let Some(data) = app.player.cached_data(id) else {
        app.player.preload(id);
        return;
    };
    let dur = data.duration_secs();
    if dur <= 0.0 {
        return;
    }
    let width = ui.available_width().min(520.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 56.0), egui::Sense::click_and_drag());
    let buckets = (width as usize).max(10);
    let peaks = app.waveform_peaks(id, &data, buckets);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(28, 29, 36));
    let pos = app.player.position(id);
    let played = pos.map(|(s, _)| (s / dur).clamp(0.0, 1.0) as f32).unwrap_or(0.0);
    let mid = rect.center().y;
    let half = rect.height() * 0.45;
    for (i, (lo, hi)) in peaks.iter().enumerate() {
        let x = rect.left() + (i as f32 + 0.5) * rect.width() / peaks.len() as f32;
        let color = if (i as f32 / peaks.len() as f32) < played { style::PLAYING } else { style::MUTED };
        let (lo, hi) = (lo.clamp(-1.0, 1.0), hi.clamp(-1.0, 1.0));
        painter.line_segment(
            [egui::pos2(x, mid - hi * half), egui::pos2(x, mid - lo * half.max(0.5))],
            egui::Stroke::new(1.0, color),
        );
    }
    if let Some((secs, paused)) = pos {
        let x = rect.left() + rect.width() * played;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(2.0, if paused { style::ACCENT } else { egui::Color32::WHITE }),
        );
        painter.text(
            rect.right_top() + egui::vec2(-6.0, 4.0),
            egui::Align2::RIGHT_TOP,
            format!("{} / {}", widgets::fmt_time(secs), widgets::fmt_time(dur)),
            egui::FontId::monospace(11.0),
            style::MUTED,
        );
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }
    if (resp.clicked() || resp.dragged())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let secs = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * dur;
        if pos.is_some() {
            app.player.seek(id, secs);
        } else if resp.clicked() {
            app.player.play(id, None);
            app.player.seek(id, secs);
        }
    }
    resp.on_hover_text(tr("Клик по волне — перемотать (или сыграть с этого места)"));
}
