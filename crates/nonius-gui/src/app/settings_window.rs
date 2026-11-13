//! Окно «Настройки»: вкладка «Проект» — то, что едет с проектом
//! (`project.yaml`), вкладка «Программа» — то, что живёт на этом компьютере
//! (`window.txt`).

use eframe::egui::{self, RichText};

use super::jobs::Done;
use super::App;
use crate::source::ProjectSettings;
use crate::texts::texts;

/// Какая вкладка окна открыта.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Project,
    Program,
}

/// Открытое окно «Настройки».
#[derive(Debug, Default)]
pub struct SettingsDialog {
    page: Page,
    name: String,
    /// Допуск, как его набрали: проверяется при сохранении.
    tolerance: String,
    /// Что было при открытии: не изменилось — сохранять нечего.
    was: Option<ProjectSettings>,
    busy: bool,
    error: Option<String>,
}

impl SettingsDialog {
    /// Окно с настройками проекта, как они есть.
    pub fn new(s: ProjectSettings) -> Self {
        Self {
            name: s.name.clone(),
            tolerance: s.tolerance_mm.to_string(),
            was: Some(s),
            ..Self::default()
        }
    }

    /// Записать не вышло: окно остаётся, причина видна в нём.
    /// Работа идёт, кнопки ждут.
    pub fn busy(&self) -> bool {
        self.busy
    }

    pub fn failed(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
    }
}

/// Допуск из поля: запятая годится так же, как точка.
fn tolerance(text: &str) -> Option<f64> {
    text.trim()
        .replace(',', ".")
        .parse()
        .ok()
        .filter(|v: &f64| v.is_finite())
}

/// Нарисовать окно, если оно открыто.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    if app.tab.settings.is_none() {
        return;
    }
    let mut close = false;
    let mut save = None;
    let modal = egui::Modal::new(egui::Id::new("settings")).show(ctx, |ui| {
        ui.set_width(420.0);
        // Одна высота для обеих вкладок: окно не прыгает при переключении.
        ui.set_min_height(300.0);
        ui.heading(t.settings_title);
        let root = app
            .tab
            .project
            .as_ref()
            .map(|p| p.root().display().to_string());
        let Some(d) = app.tab.settings.as_mut() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.selectable_value(&mut d.page, Page::Project, t.page_project);
            ui.selectable_value(&mut d.page, Page::Program, t.page_program);
        });
        ui.separator();
        match d.page {
            Page::Project => save = project_page(ui, d, root),
            Page::Program => program_page(app, ui),
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            close = ui.button(t.close).clicked();
        });
    });
    let busy = app.tab.settings.as_ref().is_some_and(|d| d.busy);
    if close || (modal.should_close() && !busy) {
        app.tab.settings = None;
    } else if let Some(s) = save {
        app.save_settings(ctx, s);
    }
}

/// Вкладка «Проект». Возвращает, что сохранить, если нажали «Сохранить».
fn project_page(
    ui: &mut egui::Ui,
    d: &mut SettingsDialog,
    root: Option<String>,
) -> Option<ProjectSettings> {
    let t = texts();
    ui.label(t.project_name);
    ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(f32::INFINITY));
    ui.add_space(4.0);
    ui.label(t.tolerance);
    ui.add(egui::TextEdit::singleline(&mut d.tolerance).desired_width(120.0));
    ui.label(RichText::new(t.tolerance_hint).weak());
    if let Some(root) = root {
        ui.add_space(4.0);
        ui.label(format!("{}: {root}", t.project_folder));
    }
    ui.add_space(4.0);
    ui.label(RichText::new(t.project_settings_where).weak());
    if let Some(e) = &d.error {
        ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
    }
    let typed = tolerance(&d.tolerance).map(|tolerance_mm| ProjectSettings {
        name: d.name.trim().to_string(),
        tolerance_mm,
    });
    let changed = typed.is_some() && typed != d.was;
    let mut save = None;
    ui.add_space(4.0);
    let button = ui.add_enabled(changed && !d.busy, egui::Button::new(t.save));
    if button.clicked() {
        save = typed;
    } else if typed.is_none() {
        ui.label(RichText::new(t.bad_tolerance).color(ui.visuals().error_fg_color));
    }
    if save.is_some() {
        d.busy = true;
        d.error = None;
    }
    save
}

/// Вкладка «Программа»: как показывать деталь, на этом компьютере.
fn program_page(app: &mut App, ui: &mut egui::Ui) {
    let t = texts();
    ui.checkbox(&mut app.look.xray, t.xray);
    ui.checkbox(&mut app.look.edges, t.edges);
    if ui
        .checkbox(&mut app.settings.wheel_reverse, t.wheel_reverse)
        .changed()
    {
        app.settings.save();
    }
    ui.add_space(4.0);
    ui.label(RichText::new(t.program_settings_where).weak());
}

impl App {
    /// Записать настройки проекта; получилось — открыть проект заново.
    fn save_settings(&mut self, ctx: &egui::Context, settings: ProjectSettings) {
        let Some(project) = self.tab.project.clone() else {
            return;
        };
        self.tab.jobs.spawn(ctx, move |_| {
            Done::SettingsSaved(project.save_settings(&settings))
        });
    }

    /// Настройки записаны: проект откроется заново, когда вкладка освободится.
    pub(super) fn settings_saved(&mut self) {
        self.tab.settings = None;
        self.tab.reopen = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerance_takes_comma() {
        assert_eq!(tolerance(" 0,02 "), Some(0.02));
        assert_eq!(tolerance("0.1"), Some(0.1));
        assert_eq!(tolerance("мало"), None);
        assert_eq!(tolerance("inf"), None);
    }
}
