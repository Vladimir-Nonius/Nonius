//! Меню правого щелчка по файлу или детали: «Открыть», «Открыть в…»,
//! «Показать в папке», у детали ещё «Заменить модель…», «Переименовать…»,
//! «Удалить…». Список программ читается в фоне при первом показе.

use std::collections::HashMap;

use eframe::egui::{self, RichText};

use super::jobs::Done;
use super::App;
use crate::source::{OpenWay, Pick, ProgramEntry};
use crate::texts::texts;

/// Программы по типу файла; `None` — ещё читаются.
pub type Programs = HashMap<String, Option<Vec<ProgramEntry>>>;

/// Над чем щёлкнули.
#[derive(Debug, Clone)]
pub struct Target {
    /// Деталь.
    pub part: String,
    /// Версия или «Сейчас».
    pub pick: Pick,
    /// Файл внутри детали.
    pub path: String,
    /// Файл можно открыть. Иначе в меню только папка.
    pub openable: bool,
    /// Щёлкнули по детали: её модель можно заменить.
    pub model_slot: bool,
}

/// Что попросили в этом кадре.
pub enum Ask {
    /// Открыть файл.
    Open(Target, OpenWay),
    /// Прочитать программы для типа файла.
    Programs(String, String),
    /// Выбрать файл и положить его в деталь.
    ReplaceModel(String),
    /// Спросить новое имя детали.
    Rename(String),
    /// Спросить, удалить ли деталь.
    Delete(String),
}

/// Тип файла для списка программ: расширение маленькими буквами.
fn kind(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default()
}

/// Повесить меню на строку.
pub fn attach(
    response: &egui::Response,
    programs: &Programs,
    target: impl FnOnce() -> Target,
    ask: &mut Option<Ask>,
) {
    response.context_menu(|ui| {
        let t = texts();
        let target = target();
        ui.set_min_width(240.0);
        if matches!(target.pick, Pick::Version(_)) {
            ui.label(RichText::new(t.menu_copy_warning).weak());
            ui.separator();
        }
        if target.openable {
            if ui.button(t.open_file).clicked() {
                *ask = Some(Ask::Open(target.clone(), OpenWay::Default));
                ui.close();
            }
            ui.menu_button(t.open_in, |ui| {
                choose_program(ui, programs, &target, ask);
            });
        }
        if ui.button(t.show_in_folder).clicked() {
            *ask = Some(Ask::Open(target.clone(), OpenWay::Folder));
            ui.close();
        }
        if target.model_slot {
            ui.separator();
            let label = if target.openable {
                t.replace_model
            } else {
                t.add_model
            };
            if ui.button(label).clicked() {
                *ask = Some(Ask::ReplaceModel(target.part.clone()));
                ui.close();
            }
            ui.separator();
            if ui.button(t.rename_part).clicked() {
                *ask = Some(Ask::Rename(target.part.clone()));
                ui.close();
            }
            if ui.button(t.delete_part).clicked() {
                *ask = Some(Ask::Delete(target.part.clone()));
                ui.close();
            }
        }
    });
}

fn choose_program(ui: &mut egui::Ui, programs: &Programs, target: &Target, ask: &mut Option<Ask>) {
    let t = texts();
    let kind = kind(&target.path);
    match programs.get(&kind) {
        None => {
            *ask = Some(Ask::Programs(kind, target.path.clone()));
            ui.spinner();
        }
        Some(None) => {
            ui.spinner();
        }
        Some(Some(list)) => {
            for program in list {
                if ui.button(&program.name).clicked() {
                    *ask = Some(Ask::Open(target.clone(), OpenWay::Program(program.clone())));
                    ui.close();
                }
            }
            if !list.is_empty() {
                ui.separator();
            }
        }
    }
    if ui.button(t.other_program).clicked() {
        *ask = Some(Ask::Open(target.clone(), OpenWay::Choose));
        ui.close();
    }
}

impl App {
    /// Выполнить то, что попросили в меню.
    pub(super) fn answer_menu(&mut self, ctx: &egui::Context, ask: Option<Ask>) {
        let Some(project) = self.project.clone() else {
            return;
        };
        match ask {
            Some(Ask::Open(target, way)) => {
                self.jobs.spawn(ctx, move |_| {
                    let folder = way == OpenWay::Folder;
                    let opened = project.open_file(&target.part, target.pick, &target.path, &way);
                    Done::FileOpened(target.pick, folder, opened)
                });
            }
            Some(Ask::Programs(kind, path)) => {
                self.programs.insert(kind.clone(), None);
                self.jobs
                    .spawn(ctx, move |_| Done::Programs(kind, project.programs(&path)));
            }
            Some(Ask::ReplaceModel(part)) => {
                let models = self.chooser.models();
                self.start_adding(models, Some(part));
            }
            Some(Ask::Rename(part)) => self.ask_rename(&part),
            Some(Ask::Delete(part)) => self.ask_delete(&part),
            None => {}
        }
    }
}
