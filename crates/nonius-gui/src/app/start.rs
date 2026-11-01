//! Начальный экран, пока проект не открыт: «Открыть проект…», «Создать
//! проект…» и перетаскивание папки или моделей в окно: модели предлагают
//! создать проект.

use eframe::egui::{self, RichText};

use super::{looks_like_dir, App};
use crate::texts::texts;

/// Нарисовать начальный экран.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    new_here(app, ctx);
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.heading(RichText::new(t.no_project).size(24.0));
            ui.add_space(16.0);
            if app.jobs.busy() {
                ui.spinner();
                return;
            }
            let open = egui::Button::new(RichText::new(t.open_project).size(16.0));
            if ui.add(open).clicked() {
                if let Some(dir) = choose_folder(&app.path_input) {
                    app.open(ctx, dir);
                }
            }
            ui.add_space(6.0);
            let create = egui::Button::new(RichText::new(t.create_project).size(16.0));
            if ui.add(create).clicked() {
                if let Some(dir) = choose_folder(&app.path_input) {
                    app.create(ctx, dir, Vec::new());
                }
            }
            ui.add_space(6.0);
            ui.label(RichText::new(t.drop_folder_hint).weak());
            if !cfg!(windows) {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 420.0).max(0.0) / 2.0);
                    let field = egui::TextEdit::singleline(&mut app.path_input)
                        .hint_text(t.folder_path)
                        .desired_width(340.0);
                    let r = ui.add(field);
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let ok = looks_like_dir(&app.path_input);
                    if (ui.add_enabled(ok, egui::Button::new("OK")).clicked() || enter) && ok {
                        let dir = app.path_input.trim().into();
                        app.open(ctx, dir);
                    }
                });
            }
            if let Some(m) = &app.message {
                ui.add_space(12.0);
                ui.label(RichText::new(m).color(ui.visuals().warn_fg_color));
            }
        });
    });
}

/// Окошко: создать проект там, откуда бросили модели?
fn new_here(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let Some((dir, models)) = app.new_here.clone() else {
        return;
    };
    let mut answer = None;
    let modal = egui::Modal::new(egui::Id::new("new-here")).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.heading(t.create_here);
        ui.add_space(6.0);
        ui.label(format!("{}: {}", t.models_found, models.len()));
        ui.label(RichText::new(dir.display().to_string()).weak());
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(t.create).clicked() {
                answer = Some(Some(dir.clone()));
            }
            if ui.button(t.other_folder).clicked() {
                answer = choose_folder(&app.path_input).map(Some);
            }
            if ui.button(t.cancel).clicked() {
                answer = Some(None);
            }
        });
    });
    match answer {
        Some(Some(dir)) => {
            app.new_here = None;
            app.create(ctx, dir, models);
        }
        Some(None) => app.new_here = None,
        None if modal.should_close() => app.new_here = None,
        None => {}
    }
}

#[cfg(windows)]
pub(super) fn choose_folder(_typed: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

/// Без системного окна выбора: путь из поля ввода.
#[cfg(not(windows))]
pub(super) fn choose_folder(typed: &str) -> Option<std::path::PathBuf> {
    looks_like_dir(typed).then(|| typed.trim().into())
}
