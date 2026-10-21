//! Вкладка «Таймеры»: запись или воспроизведение через заданное время или в заданное время.

use eframe::egui::{self, RichText};
use zv_core::i18n::tr;
use zv_core::tf;
use zv_core::timers::{TimerKind, TimerStart, TimerTask, now_unix};

use super::settings_tab::section;
use super::{ZvApp, style, widgets};
use crate::sysutil;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    Record,
    Sound,
    Queue,
}

#[derive(Debug, Clone)]
pub struct TimerForm {
    kind: FormKind,
    sound: Option<String>,
    sound_search: String,
    at_time: bool,
    after_min: u32,
    after_sec: u32,
    at_hour: u8,
    at_minute: u8,
    record_min: u32,
    record_sec: u32,
    repeat: bool,
}

impl Default for TimerForm {
    fn default() -> Self {
        Self {
            kind: FormKind::Record,
            sound: None,
            sound_search: String::new(),
            at_time: false,
            after_min: 5,
            after_sec: 0,
            at_hour: 20,
            at_minute: 0,
            record_min: 1,
            record_sec: 0,
            repeat: false,
        }
    }
}

pub fn show(app: &mut ZvApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| content(app, ui));
    });
}

fn content(app: &mut ZvApp, ui: &mut egui::Ui) {
    ui.heading(tr("Таймеры"));
    ui.label(
        RichText::new(
            tr("Запись или звук через заданное время или в заданное время суток. Записи сохраняются сами, в папку «Записи». \
             Таймеры работают, пока программа запущена (можно свернуть)."),
        )
        .color(style::MUTED),
    );
    ui.add_space(8.0);
    new_timer(app, ui);
    ui.add_space(10.0);
    list(app, ui);
}

fn new_timer(app: &mut ZvApp, ui: &mut egui::Ui) {
    let mut create = false;
    section(ui, tr("Новый таймер"), |ui| {
        let f = &mut app.timer_form;
        ui.horizontal(|ui| {
            ui.label(tr("Что сделать:"));
            ui.radio_value(&mut f.kind, FormKind::Record, tr("⏺ Записать звук"));
            ui.radio_value(&mut f.kind, FormKind::Sound, tr("▶ Сыграть звук"));
            ui.radio_value(&mut f.kind, FormKind::Queue, tr("📋 Запустить очередь"));
        });
        match f.kind {
            FormKind::Record => {
                ui.horizontal(|ui| {
                    ui.label(tr("Длительность записи:"));
                    ui.add(egui::DragValue::new(&mut f.record_min).range(0..=600).suffix(tr(" мин")));
                    ui.add(egui::DragValue::new(&mut f.record_sec).range(0..=59).suffix(tr(" с")));
                    ui.label(
                        RichText::new(tr("Источник — как выбран на вкладке «Запись».")).small().color(style::MUTED),
                    );
                });
            }
            FormKind::Sound => {
                ui.horizontal(|ui| {
                    ui.label(tr("Звук:"));
                    let current = f
                        .sound
                        .as_ref()
                        .and_then(|id| app.entries.iter().find(|e| &e.id == id))
                        .map(|e| e.name.clone())
                        .unwrap_or_else(|| tr("выберите звук").into());
                    egui::ComboBox::from_id_salt("timer_sound").width(280.0).selected_text(current).show_ui(ui, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut f.sound_search).hint_text(tr("поиск")));
                        let needle = f.sound_search.to_lowercase();
                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                            for e in app
                                .entries
                                .iter()
                                .filter(|e| needle.is_empty() || e.name.to_lowercase().contains(&needle))
                            {
                                if ui.selectable_label(f.sound.as_deref() == Some(&e.id), &e.name).clicked() {
                                    f.sound = Some(e.id.clone());
                                }
                            }
                        });
                    });
                });
            }
            FormKind::Queue => {
                ui.label(RichText::new(tf!("Сейчас в очереди: {}", app.queue.len())).color(style::MUTED));
            }
        }
        ui.horizontal(|ui| {
            ui.label(tr("Когда:"));
            ui.radio_value(&mut f.at_time, false, tr("через"));
            ui.add_enabled(!f.at_time, egui::DragValue::new(&mut f.after_min).range(0..=1440).suffix(tr(" мин")));
            ui.add_enabled(!f.at_time, egui::DragValue::new(&mut f.after_sec).range(0..=59).suffix(tr(" с")));
            ui.add_space(12.0);
            ui.radio_value(&mut f.at_time, true, tr("в"));
            ui.add_enabled(
                f.at_time,
                egui::DragValue::new(&mut f.at_hour).range(0..=23).custom_formatter(|v, _| format!("{v:02}")),
            );
            ui.label(":");
            ui.add_enabled(
                f.at_time,
                egui::DragValue::new(&mut f.at_minute).range(0..=59).custom_formatter(|v, _| format!("{v:02}")),
            );
        });
        let repeat_label = if f.at_time {
            tr("Повторять каждый день")
        } else {
            tr("Повторять с этим же интервалом")
        };
        ui.checkbox(&mut f.repeat, repeat_label);
        if ui.button(RichText::new(tr("➕ Добавить таймер")).strong()).clicked() {
            create = true;
        }
    });
    if !create {
        return;
    }
    let f = app.timer_form.clone();
    let kind = match f.kind {
        FormKind::Record => {
            let secs = f.record_min * 60 + f.record_sec;
            if secs == 0 {
                app.toast_error(tr("Укажите длительность записи"));
                return;
            }
            TimerKind::Record { secs }
        }
        FormKind::Sound => match f.sound {
            Some(id) => TimerKind::PlaySound { id },
            None => {
                app.toast_error(tr("Выберите звук"));
                return;
            }
        },
        FormKind::Queue => TimerKind::PlayQueue,
    };
    let start = if f.at_time {
        TimerStart::At { hour: f.at_hour, minute: f.at_minute }
    } else {
        let secs = f.after_min * 60 + f.after_sec;
        if secs == 0 {
            app.toast_error(tr("Укажите, через сколько сработать"));
            return;
        }
        TimerStart::After { secs }
    };
    let id = app.settings.timers.iter().map(|t| t.id).max().unwrap_or(0) + 1;
    let mut task = TimerTask { id, enabled: true, kind, start, repeat: f.repeat, next_due: None };
    task.schedule(now_unix(), sysutil::local_secs_of_day());
    app.settings.timers.push(task);
    app.mark_settings_dirty();
    app.toast(tr("Таймер добавлен"));
}

fn describe(app: &ZvApp, t: &TimerTask) -> String {
    let what = match &t.kind {
        TimerKind::Record { secs } => tf!("⏺ Записать {}", widgets::fmt_time(*secs as f64)),
        TimerKind::PlaySound { id } => {
            let name = app.entry(id).map(|e| e.name.clone()).unwrap_or_else(|| tr("(звук удалён)").into());
            format!("▶ «{name}»")
        }
        TimerKind::PlayQueue => tr("📋 Очередь").into(),
    };
    let when = match t.start {
        TimerStart::After { secs } => {
            let base = tf!("через {}", widgets::fmt_time(secs as f64));
            if t.repeat { tf!("{}, повторять", base) } else { base }
        }
        TimerStart::At { hour, minute } => {
            let base = tf!("в {}:{}", format!("{:02}", hour), format!("{:02}", minute));
            if t.repeat { tf!("{}, каждый день", base) } else { base }
        }
    };
    format!("{what} — {when}")
}

fn list(app: &mut ZvApp, ui: &mut egui::Ui) {
    let mut remove = None;
    let mut toggled = None;
    let now = now_unix();
    let timers = app.settings.timers.clone();
    section(ui, tr("Таймеры"), |ui| {
        if timers.is_empty() {
            ui.label(RichText::new(tr("Пока нет таймеров")).color(style::MUTED));
        }
        for (i, t) in timers.iter().enumerate() {
            ui.horizontal(|ui| {
                let mut on = t.enabled;
                if ui.checkbox(&mut on, "").on_hover_text(tr("Включён")).changed() {
                    toggled = Some((i, on));
                }
                let mut text = RichText::new(describe(app, t));
                if !t.enabled {
                    text = text.color(style::MUTED);
                }
                ui.label(text);
                if t.enabled
                    && let Some(due) = t.next_due
                {
                    let left = due.saturating_sub(now);
                    let h = left / 3600;
                    let s = if h > 0 {
                        tf!("сработает через {} ч {} мин", h, format!("{:02}", (left / 60) % 60))
                    } else {
                        tf!("сработает через {}", widgets::fmt_time(left as f64))
                    };
                    ui.label(RichText::new(s).color(style::ACCENT));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(RichText::new("🗑").color(style::DANGER)).on_hover_text(tr("Удалить")).clicked()
                    {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(until) = app.timer_record_until {
            ui.label(
                RichText::new(tf!(
                    "● Идёт запись по таймеру, осталось {}",
                    widgets::fmt_time(until.saturating_sub(now) as f64)
                ))
                .color(style::DANGER),
            );
        }
    });
    if let Some((i, on)) = toggled {
        let local = sysutil::local_secs_of_day();
        let t = &mut app.settings.timers[i];
        t.enabled = on;
        if on {
            t.schedule(now, local);
        } else {
            t.next_due = None;
        }
        app.mark_settings_dirty();
    }
    if let Some(i) = remove {
        app.settings.timers.remove(i);
        app.mark_settings_dirty();
    }
}
