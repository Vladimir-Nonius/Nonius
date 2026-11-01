//! Меню «Файл»: открыть и создать проект, добавить модели, новая деталь.

use std::path::{Path, PathBuf};

use eframe::egui;
use nonius_core::project::PROJECT_FILE;

use super::new_part::NewPart;
use super::start::choose_folder;
use super::App;
use crate::texts::texts;

/// Файлы моделей, которые принимает проект.
const MODEL_FILES: [&str; 4] = ["step", "stp", "sldprt", "sldasm"];

/// Насколько глубоко искать модели в перетащенной папке.
const SEARCH_DEPTH: usize = 6;

/// Нарисовать меню.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    egui::TopBottomPanel::top("file-menu").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(t.menu_file, |ui| {
                ui.set_min_width(220.0);
                if ui.button(t.add_models).clicked() {
                    ui.close();
                    app.start_adding(ctx, choose_models(), None);
                }
                if ui.button(t.add_folder).clicked() {
                    ui.close();
                    if let Some(dir) = choose_folder(&app.path_input) {
                        app.start_adding(ctx, models_in(&dir), None);
                    }
                }
                if ui.button(t.new_part).clicked() {
                    ui.close();
                    app.new_part = Some(NewPart::default());
                }
                ui.separator();
                if ui.button(t.open_project).clicked() {
                    ui.close();
                    if let Some(dir) = choose_folder(&app.path_input) {
                        app.open(ctx, dir);
                    }
                }
                if ui.button(t.create_project).clicked() {
                    ui.close();
                    if let Some(dir) = choose_folder(&app.path_input) {
                        app.create(ctx, dir, Vec::new());
                    }
                }
            });
        });
    });
}

/// Файл модели, а не что-то другое. Временные файлы SolidWorks (`~$…`) — нет.
pub fn is_model(path: &Path) -> bool {
    let temporary = path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("~$"));
    !temporary
        && path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| MODEL_FILES.contains(&e.to_ascii_lowercase().as_str()))
}

/// Эта папка или файл уже в проекте.
pub fn in_project(path: &Path) -> bool {
    path.ancestors().any(|d| d.join(PROJECT_FILE).is_file())
}

/// Модели в папке и её подпапках по имени. Скрытые папки не смотрим.
pub fn models_in(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            let hidden = path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'));
            if path.is_dir() {
                if depth > 0 && !hidden {
                    walk(&path, depth - 1, out);
                }
            } else if is_model(&path) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, SEARCH_DEPTH, &mut out);
    out
}

#[cfg(windows)]
pub(super) fn choose_models() -> Vec<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(texts().model_files, &MODEL_FILES)
        .pick_files()
        .unwrap_or_default()
}

/// Без системного окна выбора файл не выбрать: модель можно перетащить.
#[cfg(not(windows))]
pub(super) fn choose_models() -> Vec<PathBuf> {
    Vec::new()
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
        assert!(!is_model(Path::new("~$корпус.sldprt")));
    }

    #[test]
    fn models_are_found_in_subfolders() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        std::fs::create_dir_all(d.join("узел/крепёж")).unwrap();
        std::fs::create_dir_all(d.join(".git")).unwrap();
        for file in ["а.step", "узел/б.sldprt", "узел/крепёж/в.stp", "узел/г.txt"]
        {
            std::fs::write(d.join(file), "x").unwrap();
        }
        std::fs::write(d.join(".git/д.step"), "x").unwrap();
        let names: Vec<_> = models_in(d)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["а.step", "б.sldprt", "в.stp"]);
    }
}
