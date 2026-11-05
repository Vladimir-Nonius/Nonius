//! Меню окна: «Файл» (проект и модели), «Деталь» (что сделать с выбранной),
//! «Вид» (как показывать).

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText};
use nonius_core::project::PROJECT_FILE;

use super::new_part::NewPart;
use super::settings_window::SettingsDialog;
use super::App;
use crate::source::{Other, Pick};
use crate::texts::texts;

/// Файлы моделей, которые принимает проект.
pub(super) const MODEL_FILES: [&str; 4] = ["step", "stp", "sldprt", "sldasm"];

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
                    app.start_adding(app.chooser.models(), None);
                }
                if ui.button(t.add_folder).clicked() {
                    ui.close();
                    if let Some(dir) = app.chooser.folder(&app.path_input) {
                        app.start_adding(models_in(&dir), None);
                    }
                }
                if ui.button(t.new_part).clicked() {
                    ui.close();
                    app.tab.new_part = Some(NewPart::in_folder(&app.tab.place));
                }
                if ui.button(t.settings).clicked() {
                    ui.close();
                    if let Some(p) = &app.tab.project {
                        let dialog = SettingsDialog::new(p.settings());
                        app.tab.settings = Some(dialog);
                    }
                }
                if ui.button(t.deleted_parts).clicked() {
                    ui.close();
                    app.show_deleted(ctx);
                }
                ui.separator();
                if ui.button(t.new_project).clicked() {
                    ui.close();
                    app.new_project(ctx);
                }
                if ui.button(t.open_project).clicked() {
                    ui.close();
                    if let Some(dir) = app.chooser.folder(&app.path_input) {
                        app.open(ctx, dir);
                    }
                }
            });
            ui.add_enabled_ui(app.tab.part.is_some(), |ui| part_menu(app, ui));
            view_menu(app, ui);
        });
    });
}

/// Меню выбранной детали. Действия с версией — про показанную как «Стало».
fn part_menu(app: &mut App, ui: &mut egui::Ui) {
    let t = texts();
    let ctx = ui.ctx().clone();
    let version = match app.tab.picks.1 {
        Pick::Version(n) => Some(n),
        Pick::Current => None,
    };
    let latest = app.tab.versions.first().map(|v| v.number);
    let others: Vec<String> = app
        .tab
        .parts
        .iter()
        .map(|p| p.name.clone())
        .filter(|n| Some(n) != app.tab.part.as_ref())
        .collect();
    ui.menu_button(t.menu_part, |ui| {
        ui.set_min_width(240.0);
        let Some(part) = app.tab.part.clone() else {
            return;
        };
        // Меню — про открытую в 3D, а выделено может быть другое.
        let opened = format!("{} «{part}»", t.opened_part);
        ui.label(RichText::new(opened).weak());
        ui.separator();
        if ui
            .add_enabled(app.part_dirty(), egui::Button::new(t.save_version))
            .clicked()
        {
            ui.close();
            app.start_save(&ctx);
        }
        let model = app.tab.parts.iter().find(|p| p.name == part);
        let replace = model.is_some_and(|p| p.model.is_some());
        let label = if replace { t.add_version } else { t.add_model };
        if ui.button(label).clicked() {
            ui.close();
            app.start_adding(app.chooser.models(), Some(part.clone()));
        }
        if ui.button(t.add_file).clicked() {
            ui.close();
            app.add_files(&ctx, part.clone());
        }
        ui.separator();
        if ui.button(t.compare_file).clicked() {
            ui.close();
            if let Some(file) = app.chooser.step() {
                app.compare_with(&ctx, Other::File(file));
            }
        }
        ui.add_enabled_ui(!others.is_empty(), |ui| {
            ui.menu_button(t.compare_part, |ui| {
                for other in others {
                    if ui.button(&other).clicked() {
                        ui.close();
                        app.compare_with(&ctx, Other::Part(other));
                    }
                }
            });
        });
        ui.separator();
        let old = version.filter(|n| Some(*n) != latest);
        if ui
            .add_enabled(old.is_some(), egui::Button::new(t.restore_version))
            .clicked()
        {
            ui.close();
            app.ask_restore(old);
        }
        if ui
            .add_enabled(version.is_some(), egui::Button::new(t.export_version))
            .clicked()
        {
            ui.close();
            app.export_version(&ctx, version);
        }
        ui.separator();
        if ui.button(t.rename_part).clicked() {
            ui.close();
            app.ask_rename(&part);
        }
        if ui.button(t.delete_part).clicked() {
            ui.close();
            app.ask_delete(&part);
        }
    });
}

/// Меню «Вид»: как показывать деталь.
fn view_menu(app: &mut App, ui: &mut egui::Ui) {
    let t = texts();
    ui.menu_button(t.menu_view, |ui| {
        ui.set_min_width(220.0);
        ui.checkbox(&mut app.look.xray, t.xray);
        ui.checkbox(&mut app.look.edges, t.edges);
        if ui
            .checkbox(&mut app.settings.wheel_reverse, t.wheel_reverse)
            .changed()
        {
            app.settings.save();
        }
        ui.separator();
        if ui
            .add_enabled(
                app.viewport.model().is_some(),
                egui::Button::new(t.save_picture),
            )
            .clicked()
        {
            ui.close();
            super::toolbar::save_picture(app);
        }
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
