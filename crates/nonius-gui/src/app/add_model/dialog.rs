//! Само окошко «Добавить модель»: файл, куда он ляжет, описание, кнопки.

use eframe::egui::{self, RichText};

use super::super::model_names::{
    default_message, elsewhere, next_version, step_stays, where_words, Place,
};
use super::super::App;
use crate::texts::texts;

/// Нарисовать окошко, если оно открыто.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let parts = app.tab.parts.clone();
    let Some(d) = app.tab.add.as_mut() else {
        return;
    };
    let Some(file) = d.file().cloned() else {
        return;
    };
    let mut add = false;
    let mut all = false;
    let mut skip = false;
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("add-model")).show(ctx, |ui| {
        ui.set_width(460.0);
        let done = d.total - d.queue.len() + 1;
        ui.heading(if d.total > 1 {
            format!("{} {done} {} {}", t.add_model_title, t.place_of, d.total)
        } else {
            t.add_model_title.to_string()
        });
        let shown = file.file_name().unwrap_or_default().to_string_lossy();
        ui.label(RichText::new(shown).weak());
        ui.add_space(6.0);
        if !d.fixed {
            let before = d.target.clone();
            let selected = d.target.as_deref().unwrap_or(t.part_new);
            ui.label(t.add_to);
            egui::ComboBox::from_id_salt("add-target")
                .selected_text(selected)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut d.target, None, t.part_new);
                    for p in &parts {
                        ui.selectable_value(&mut d.target, Some(p.name.clone()), &p.name);
                    }
                });
            if d.target != before {
                d.message = default_message(d.target.as_deref(), &parts);
            }
        }
        if d.target.is_none() {
            ui.label(t.part_name);
            ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(f32::INFINITY));
        }
        let place = d.place(&parts);
        match &place {
            Place::Existing(part) => {
                let next = next_version(part, &parts);
                ui.label(RichText::new(format!("{} {next} «{part}»", t.will_be_version)).strong());
            }
            Place::New(name) => {
                let mut text = format!("{} «{name}»", t.will_be_part);
                if let Some(folder) = &d.folder {
                    text = format!("{text} {}", where_words(folder));
                }
                ui.label(RichText::new(text).strong());
            }
            Place::Nothing => {
                ui.label(RichText::new(t.bad_name).color(ui.visuals().warn_fg_color));
            }
        }
        if let Some(text) = elsewhere(&place, d.folder.as_deref(), &parts) {
            ui.label(RichText::new(text).color(ui.visuals().warn_fg_color));
        }
        if step_stays(&file, &place, &parts) {
            ui.label(RichText::new(t.step_stays).color(ui.visuals().warn_fg_color));
        }
        ui.add_space(6.0);
        ui.label(format!("{} *", t.what_why));
        let edit = ui.add(
            egui::TextEdit::singleline(&mut d.message)
                .desired_width(f32::INFINITY)
                .hint_text(t.add_model_hint),
        );
        if !d.focused {
            edit.request_focus();
            d.focused = true;
        }
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if let Some(e) = &d.error {
            ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
        }
        ui.add_space(6.0);
        let ready = !d.busy && place != Place::Nothing && !d.message.trim().is_empty();
        ui.horizontal(|ui| {
            let label = if d.busy { t.importing } else { t.add };
            let button = ui.add_enabled(ready, egui::Button::new(label));
            add = ready && (button.clicked() || enter);
            if d.queue.len() > 1 {
                let label = format!("{} ({})", t.add_all, d.queue.len());
                all = ui.add_enabled(ready, egui::Button::new(label)).clicked();
                skip = ui.add_enabled(!d.busy, egui::Button::new(t.skip)).clicked();
            }
            close = ui
                .add_enabled(!d.busy, egui::Button::new(t.cancel))
                .clicked();
        });
    });
    if close || (modal.should_close() && !d.busy) {
        app.finish_adding(ctx);
    } else if skip {
        d.queue.pop_front();
        d.prepare(&parts);
    } else if add || all {
        d.all = all;
        app.add_current(ctx);
    }
}
