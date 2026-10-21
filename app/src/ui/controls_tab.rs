//! Настройки → Управление: схема клавиш, нумпад, пропуск нажатий, мышь.

use eframe::egui::{self, RichText};
use zv_core::controls::{Action, ActionGroup, CUSTOM_PRESET, presets};
use zv_core::i18n::tr;
use zv_core::settings::NumpadModifier;
use zv_core::tf;

use super::settings_tab::section;
use super::{CaptureTarget, ZvApp, style, widgets};

pub fn show(app: &mut ZvApp, ui: &mut egui::Ui) {
    let mut changed = false;

    section(ui, tr("Схема управления"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for p in presets() {
                let active = app.settings.controls.preset == p.name;
                if ui.selectable_label(active, tr(p.name)).on_hover_text(tr(p.description)).clicked() {
                    let c = &mut app.settings.controls;
                    c.preset = p.name.to_string();
                    c.bindings = p.bindings();
                    c.pass_through = p.pass_through;
                    match p.numpad {
                        Some(m) => {
                            app.settings.numpad.enabled = true;
                            app.settings.numpad.modifier = m;
                        }
                        None => app.settings.numpad.enabled = false,
                    }
                    changed = true;
                    app.toast(tf!("Схема «{}» применена", tr(p.name)));
                }
            }
            if app.settings.controls.preset == CUSTOM_PRESET {
                ui.label(RichText::new(tr("● Своя (изменена вручную)")).color(style::ACCENT));
            }
        });
        let desc = presets().iter().find(|p| p.name == app.settings.controls.preset).map(|p| tr(p.description));
        if let Some(d) = desc {
            ui.label(RichText::new(d).small().color(style::MUTED));
        }
    });
    ui.add_space(10.0);

    section(ui, tr("Номер звука на нумпаде (как в Soundpad)"), |ui| {
        let n = &mut app.settings.numpad;
        changed |= ui.checkbox(&mut n.enabled, tr("Включить набор номера")).changed();
        ui.add_enabled_ui(n.enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("Держать:"));
                for m in NumpadModifier::ALL {
                    changed |= ui.radio_value(&mut n.modifier, m, m.label()).changed();
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("Подсказок в оверлее:"));
                changed |= ui.add(egui::Slider::new(&mut n.max_hints, 3..=20)).changed();
            });
            changed |= ui
                .checkbox(&mut n.auto_assign, tr("Сам выдавать номера новым звукам (1, 2, 3… — номера не сдвигаются)"))
                .changed();
            let m = n.modifier.label();
            ui.label(
                RichText::new(tf!("Как пользоваться: зажмите {}, наберите номер цифрами на нумпаде (NumLock должен быть включён), отпустите {}. \
                     Пока набираете, оверлей показывает подходящие звуки. {}+0 — повторить последний, {}+точка — отмена. \
                     Номер звука меняется в его карточке.", m, m, m, m))
                .small()
                .color(style::MUTED),
            );
        });
    });
    ui.add_space(10.0);

    section(ui, tr("Поведение клавиш"), |ui| {
        let c = &mut app.settings.controls;
        changed |= ui.checkbox(&mut c.sound_hotkeys_enabled, tr("Клавиши звуков включены")).changed();
        changed |= ui
            .checkbox(&mut c.pass_through, tr("Пропускать нажатия клавиш звуков в игру"))
            .on_hover_text(tr("Игра тоже получит нажатие. Работает через опрос клавиш, без перехватчиков."))
            .changed();
        ui.label(
            RichText::new(tr("Без пропуска назначенная клавиша «съедается»: звук играет, а игра нажатия не видит. \
                 С пропуском — видят оба. Действия (стоп, громкость…) не пропускаются никогда."))
            .small()
            .color(style::MUTED),
        );
        ui.horizontal(|ui| {
            ui.label(tr("Шаг громкости для клавиш «громче/тише»:"));
            changed |= ui
                .add(
                    egui::Slider::new(&mut c.volume_step, 0.01..=0.25)
                        .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                )
                .changed();
        });
        changed |= ui.checkbox(&mut c.enter_plays, tr("Enter в списке звуков — играть выбранный")).changed();
    });
    ui.add_space(10.0);

    section(ui, tr("Клавиши действий"), |ui| {
        ui.label(
            RichText::new(
                tr("Нажмите на кнопку и затем сочетание клавиш или кнопку мыши (средняя, Mouse4, Mouse5; левая/правая — с Ctrl/Alt/Shift)."),
            )
            .small()
            .color(style::MUTED),
        );
        let mut capture = None;
        for group in [ActionGroup::Playback, ActionGroup::Mode, ActionGroup::Volume, ActionGroup::Other] {
            ui.add_space(4.0);
            ui.label(RichText::new(group.label()).strong().color(style::ACCENT));
            egui::Grid::new(format!("act_{}", group.label())).num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
                for a in Action::ALL.into_iter().filter(|a| a.group() == group) {
                    ui.label(a.label());
                    let key = app.settings.controls.key(a).to_string();
                    let err = app.hotkeys.errors.get(&key);
                    if widgets::hotkey_button(ui, (!key.is_empty()).then_some(key.as_str()), err).clicked() {
                        capture = Some(a);
                    }
                    if !key.is_empty() && ui.small_button("✖").on_hover_text(tr("Убрать")).clicked() {
                        app.settings.controls.set_key(a, String::new());
                        changed = true;
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        }
        if let Some(a) = capture {
            app.capture = Some(CaptureTarget::Action(a));
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                tr("Клавиши звуков назначаются на вкладке «Звуки». Если клавиша красная — она занята другой программой. \
                 В играх, запущенных от имени администратора, клавиши работают, только если программа тоже запущена от администратора."),
            )
            .small()
            .color(style::MUTED),
        );
    });

    ui.add_space(10.0);
    section(ui, tr("🎲 Случайный звук из тега"), |ui| {
        ui.label(
            RichText::new(
                tr("Клавиша играет случайный звук с выбранным тегом (не повторяя только что игравший). Назначается правой кнопкой по тегу на вкладке «Звуки» или здесь."),
            )
            .small()
            .color(style::MUTED),
        );
        let tags = app.meta.all_tags();
        if tags.is_empty() {
            ui.label(RichText::new(tr("Тегов пока нет — добавьте их в карточке звука.")).color(style::MUTED));
        }
        let mut capture = None;
        egui::Grid::new("tag_random").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
            for (tag, count) in &tags {
                ui.label(format!("#{tag}  ({count})"));
                let key = app.settings.controls.tag_random.get(tag).cloned().unwrap_or_default();
                let err = app.hotkeys.errors.get(&key);
                if widgets::hotkey_button(ui, (!key.is_empty()).then_some(key.as_str()), err).clicked() {
                    capture = Some(tag.clone());
                }
                if !key.is_empty() && ui.small_button("✖").on_hover_text(tr("Убрать")).clicked() {
                    app.settings.controls.set_tag_key(tag, String::new());
                    changed = true;
                } else {
                    ui.label("");
                }
                ui.end_row();
            }
        });
        if let Some(t) = capture {
            app.capture = Some(CaptureTarget::TagRandom(t));
        }
    });

    ui.add_space(10.0);
    section(ui, tr("📻 Автонажатие клавиши рации"), |ui| {
        ui.label(
            RichText::new(
                tr("Если в игре или Discord включён режим «рация» (push-to-talk), звук «в микрофон» не слышен, пока клавиша не нажата. Программа может сама держать её нажатой, пока играет звук."),
            )
            .small()
            .color(style::MUTED),
        );
        let p = &mut app.settings.auto_ptt;
        changed |= ui.checkbox(&mut p.enabled, tr("Нажимать клавишу рации, пока звук играет «в микрофон»")).changed();
        ui.add_enabled_ui(p.enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("Клавиша рации:"));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut p.key).desired_width(170.0).hint_text(tr("например V или F13")),
                );
                if resp.changed() {
                    changed = true;
                }
                ui.label(
                    RichText::new(tr("одна клавиша, как в игре; можно Mouse4 / Mouse5")).small().color(style::MUTED),
                );
            });
            if !p.key.trim().is_empty()
                && let Err(e) = crate::ptt::validate(&p.key)
            {
                ui.label(RichText::new(format!("⚠ {e}")).color(style::DANGER));
            }
            ui.horizontal(|ui| {
                ui.label(tr("Отпускать через:"));
                changed |= ui.add(egui::Slider::new(&mut p.release_ms, 0..=2000).suffix(tr(" мс"))).changed();
            });
            if app.ptt.is_pressed() {
                ui.label(RichText::new(tr("● клавиша рации сейчас нажата")).color(style::ACCENT));
            }
        });
        ui.label(
            RichText::new(
                tr("⚠ Нажатие получает активное окно. Выберите клавишу, которая в игре ничего больше не делает. Это искусственное нажатие (как у экранной клавиатуры): обычные игры его принимают, но строгие античиты могут его игнорировать или не любить — в соревновательных играх лучше настройте в Discord «Активация по голосу» для CABLE Output."),
            )
            .small()
            .color(style::MUTED),
        );
    });

    if changed {
        if app.settings.numpad.auto_assign {
            let ids: Vec<String> = app.entries.iter().map(|e| e.id.clone()).collect();
            if app.meta.assign_codes(ids.iter().map(String::as_str)) {
                app.mark_meta_dirty();
            }
        }
        app.mark_settings_dirty();
        app.update_numpad_codes();
        app.apply_audio_settings();
    }
}
