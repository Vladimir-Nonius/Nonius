//! Папки в дереве: окошко «Новая папка» и перенос детали в другую папку.
//! Папка в дереве — это папка на диске, отдельно она нигде не записана.

use eframe::egui::{self, RichText};

use super::jobs::Done;
use super::App;
use crate::texts::texts;

/// Открытое окошко «Новая папка».
#[derive(Debug, Default)]
pub struct NewFolder {
    name: String,
    /// Внутри какой папки: путь от корня, пустой — корень.
    parent: String,
    busy: bool,
    error: Option<String>,
    focused: bool,
}

impl NewFolder {
    /// Окошко для папки внутри `parent`.
    pub fn in_folder(parent: &str) -> Self {
        Self {
            parent: parent.to_string(),
            ..Self::default()
        }
    }

    /// Создать не вышло: окошко остаётся, причина видна в нём.
    pub fn failed(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
        self.focused = false;
    }
}

/// «В папке «Стена/Рейка»» или «В папке: в корне проекта» — куда что ляжет.
pub fn place_words(folder: &str) -> String {
    let t = texts();
    if folder.is_empty() {
        format!("{}: {}", t.in_folder, t.in_root)
    } else {
        format!("{} «{folder}»", t.in_folder)
    }
}

/// Путь новой папки: внутри `parent`, лишние `/` и пробелы убраны.
fn joined(parent: &str, name: &str) -> String {
    parent
        .split('/')
        .chain(name.split(['/', '\\']))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// Нарисовать окошко, если оно открыто.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let Some(dialog) = app.tab.new_folder.as_mut() else {
        return;
    };
    let mut create = false;
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("new-folder")).show(ctx, |ui| {
        ui.set_width(340.0);
        ui.heading(t.new_folder_title);
        ui.add_space(6.0);
        ui.label(t.folder_name);
        let edit =
            ui.add(egui::TextEdit::singleline(&mut dialog.name).desired_width(f32::INFINITY));
        if !dialog.focused {
            edit.request_focus();
            dialog.focused = true;
        }
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.label(RichText::new(t.folder_name_hint).weak());
        ui.label(RichText::new(place_words(&dialog.parent)).weak());
        if let Some(e) = &dialog.error {
            ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
        }
        ui.add_space(6.0);
        let ready = !dialog.busy && !dialog.name.trim().is_empty();
        ui.horizontal(|ui| {
            let button = ui.add_enabled(ready, egui::Button::new(t.create));
            create = ready && (button.clicked() || enter);
            close = ui.button(t.cancel).clicked();
        });
    });
    if close || (modal.should_close() && !dialog.busy) {
        app.tab.new_folder = None;
    } else if create {
        dialog.busy = true;
        dialog.error = None;
        let path = joined(&dialog.parent, &dialog.name);
        app.create_folder(ctx, path);
    }
}

impl App {
    /// Создать папку из окошка «Новая папка».
    fn create_folder(&mut self, ctx: &egui::Context, path: String) {
        let Some(project) = self.tab.project.clone() else {
            return;
        };
        self.tab.jobs.spawn(ctx, move |_| {
            let made = project.new_folder(&path);
            Done::FolderCreated(path, made)
        });
    }

    /// Папка создана: она раскрыта и новые детали ложатся в неё.
    pub(super) fn folder_created(&mut self, ctx: &egui::Context, path: String) {
        self.tab.new_folder = None;
        self.tab.notice = Some(format!("{} «{path}»", texts().folder_created));
        self.unfold_to(&path);
        self.tab.place = path;
        self.reload(ctx);
    }

    /// Деталь перенесена: показать её на новом месте.
    pub(super) fn part_moved(&mut self, ctx: &egui::Context, part: String, folder: String) {
        let t = texts();
        let done = if folder.is_empty() {
            t.moved_to_root.to_string()
        } else {
            format!("{} «{folder}»", t.moved_to)
        };
        self.unfold_to(&folder);
        self.tab.notice = Some(format!("«{part}» {done}"));
        self.reload(ctx);
    }

    /// Раскрыть папку и все папки над ней.
    pub(super) fn unfold_to(&mut self, folder: &str) {
        let mut path = String::new();
        for name in folder.split('/').filter(|s| !s.is_empty()) {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(name);
            self.tab.folded.remove(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_folder_path_is_cleaned() {
        assert_eq!(joined("", " Стена "), "Стена");
        assert_eq!(joined("Стена", "Рейка/Крюки"), "Стена/Рейка/Крюки");
        assert_eq!(joined("Стена", "\\Рейка//"), "Стена/Рейка");
    }
}
