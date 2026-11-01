//! Полоса над всем окном: открыть и создать проект, добавить деталь и модель.

use std::path::{Path, PathBuf};

use eframe::egui;

use super::new_part::NewPart;
use super::start::choose_folder;
use super::App;
use crate::texts::texts;

/// Файлы моделей, которые принимает проект.
const MODEL_FILES: [&str; 4] = ["step", "stp", "sldprt", "sldasm"];

/// Нарисовать полосу.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    egui::TopBottomPanel::top("project-bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button(t.open_project).clicked() {
                if let Some(dir) = choose_folder(&app.path_input) {
                    app.open(ctx, dir);
                }
            }
            if ui.button(t.create_project).clicked() {
                if let Some(dir) = choose_folder(&app.path_input) {
                    app.create(ctx, dir);
                }
            }
            ui.separator();
            if ui.button(t.new_part).clicked() {
                app.new_part = Some(NewPart::default());
            }
            if ui.button(t.add_model).clicked() {
                if let Some(file) = choose_model() {
                    app.import(ctx, file, None);
                }
            }
        });
    });
}

/// Файл модели, а не что-то другое.
pub fn is_model(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MODEL_FILES.contains(&e.to_ascii_lowercase().as_str()))
}

#[cfg(windows)]
pub(super) fn choose_model() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(texts().model_files, &MODEL_FILES)
        .pick_file()
}

/// Без системного окна выбора файл не выбрать: модель можно перетащить.
#[cfg(not(windows))]
pub(super) fn choose_model() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_are_known_by_extension() {
        assert!(is_model(Path::new("кронштейн.STEP")));
        assert!(is_model(Path::new("корпус.sldasm")));
        assert!(!is_model(Path::new("заметка.txt")));
        assert!(!is_model(Path::new("папка")));
    }
}
