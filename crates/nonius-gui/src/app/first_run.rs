//! Первый запуск: без git истории версий нет, а без имени её не подписать.
//! Спрашиваем словами здесь, а не командами в ошибке потом.

use std::sync::Arc;

use eframe::egui::{self, RichText};

use super::App;
use crate::source::{Machine, Readiness};
use crate::texts::texts;

/// Сайт, откуда ставить Git.
const GIT_SITE: &str = "https://git-scm.com/download/win";

/// Открытое окошко.
pub struct FirstRun {
    machine: Arc<dyn Machine>,
    ready: Readiness,
    name: String,
    error: Option<String>,
    focused: bool,
}

impl FirstRun {
    /// Окошко, если на компьютере чего-то не хватает.
    pub fn check(machine: Arc<dyn Machine>, ready: Readiness) -> Option<Self> {
        (!ready.git || !ready.author).then(|| Self {
            machine,
            ready,
            name: String::new(),
            error: None,
            focused: false,
        })
    }
}

/// Нарисовать окошко, если оно открыто.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let Some(d) = app.first_run.as_mut() else {
        return;
    };
    let mut done = false;
    egui::Modal::new(egui::Id::new("first-run")).show(ctx, |ui| {
        ui.set_width(420.0);
        if d.ready.git {
            ui.heading(t.who_are_you);
            ui.add_space(6.0);
            ui.label(t.who_hint);
            let edit = ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(f32::INFINITY));
            if !d.focused {
                edit.request_focus();
                d.focused = true;
            }
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if !d.ready.lfs {
                ui.label(RichText::new(t.no_lfs).weak());
            }
            if let Some(e) = &d.error {
                ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
            }
            ui.add_space(6.0);
            let ready = !d.name.trim().is_empty();
            let button = ui.add_enabled(ready, egui::Button::new(t.remember));
            if ready && (button.clicked() || enter) {
                match d.machine.set_author(&d.name) {
                    Ok(()) => done = true,
                    Err(e) => d.error = Some(format!("{e:#}")),
                }
            }
        } else {
            ui.heading(t.git_needed);
            ui.add_space(6.0);
            ui.label(t.git_why);
            ui.hyperlink(GIT_SITE);
            ui.add_space(6.0);
            if ui.button(t.check_again).clicked() {
                d.ready = d.machine.readiness();
                done = d.ready.git && d.ready.author;
            }
        }
    });
    if done {
        app.first_run = None;
    }
}
