//! Начальный экран, пока проект не открыт: кнопка «Открыть папку проекта…»
//! и перетаскивание папки в окно.

use eframe::egui::{self, RichText};

use super::{looks_like_dir, App};
use crate::texts::texts;

/// Нарисовать начальный экран.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
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

#[cfg(windows)]
fn choose_folder(_typed: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

/// Без системного окна выбора: путь из поля ввода.
#[cfg(not(windows))]
fn choose_folder(typed: &str) -> Option<std::path::PathBuf> {
    looks_like_dir(typed).then(|| typed.trim().into())
}
