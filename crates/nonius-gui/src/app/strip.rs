//! Вкладки проектов, как в браузере: полоса над окном, «+» открывает или
//! создаёт проект в новой вкладке, крестик закрывает. Открытые вкладки
//! помнятся между запусками.
//!
//! Открытая вкладка лежит в `App::tab`, остальные — в `App::tabs`; на месте
//! открытой там пустышка. Переключение — обмен двух вкладок местами.

use std::path::Path;

use eframe::egui::{self, RichText};

use super::project_tab::ProjectTab;
use super::App;
use crate::source::ProjectSource;
use crate::texts::texts;

/// Нарисовать полосу вкладок.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let mut switch = None;
    let mut close = None;
    egui::TopBottomPanel::top("project-tabs").show(ctx, |ui| {
        ui.horizontal(|ui| {
            for i in 0..app.tabs.len() {
                let tab = app.tab_at(i);
                let active = i == app.at;
                let title = tab_title(tab);
                let busy = tab.jobs.busy() && tab.project.is_none();
                ui.group(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let text = if active {
                        RichText::new(&title).strong()
                    } else {
                        RichText::new(&title)
                    };
                    if ui.selectable_label(active, text).clicked() && !active {
                        switch = Some(i);
                    }
                    if busy {
                        ui.spinner();
                    }
                    let x = ui.small_button("×").on_hover_text(t.close_tab);
                    if x.clicked() {
                        close = Some(i);
                    }
                });
            }
            ui.menu_button(RichText::new("+").strong(), |ui| {
                if ui.button(t.open_project).clicked() {
                    ui.close();
                    if let Some(dir) = app.chooser.folder(&app.path_input) {
                        app.open(ctx, dir);
                    }
                }
                if ui.button(t.create_project).clicked() {
                    ui.close();
                    if let Some(dir) = app.chooser.folder(&app.path_input) {
                        app.create(ctx, dir, Vec::new());
                    }
                }
            })
            .response
            .on_hover_text(t.new_tab);
        });
    });
    if let Some(i) = switch {
        app.switch_to(ctx, i);
    }
    if let Some(i) = close {
        app.close_tab(ctx, i);
    }
}

/// Надпись на вкладке: имя проекта.
fn tab_title(tab: &ProjectTab) -> String {
    match &tab.project {
        Some(p) => p.title(),
        None => texts().new_tab.to_string(),
    }
}

impl App {
    /// Вкладка на месте `i`.
    fn tab_at(&self, i: usize) -> &ProjectTab {
        if i == self.at {
            &self.tab
        } else {
            &self.tabs[i]
        }
    }

    /// Проект открылся: в этой вкладке, если она пустая, иначе в новой.
    /// Уже открытый проект не открывается второй раз — окно переходит к нему.
    pub(super) fn got_project(&mut self, ctx: &egui::Context, project: Box<dyn ProjectSource>) {
        let root = project.root().to_path_buf();
        if let Some(i) = (0..self.tabs.len()).find(|&i| holds(self.tab_at(i), &root)) {
            self.tab.after_open.clear();
            self.switch_to(ctx, i);
            return;
        }
        if self.tab.project.is_some() {
            let after = std::mem::take(&mut self.tab.after_open);
            self.add_tab(ctx);
            self.tab.after_open = after;
        }
        self.set_project(ctx, project);
    }

    /// Новая пустая вкладка в конце; она становится открытой.
    pub(super) fn add_tab(&mut self, ctx: &egui::Context) {
        std::mem::swap(&mut self.tab, &mut self.tabs[self.at]);
        self.tabs.push(ProjectTab::default());
        self.at = self.tabs.len() - 1;
        std::mem::swap(&mut self.tab, &mut self.tabs[self.at]);
        self.shown_again(ctx);
    }

    /// Открыть вкладку `i`.
    pub(super) fn switch_to(&mut self, ctx: &egui::Context, i: usize) {
        if i == self.at || i >= self.tabs.len() {
            return;
        }
        std::mem::swap(&mut self.tab, &mut self.tabs[self.at]);
        self.at = i;
        std::mem::swap(&mut self.tab, &mut self.tabs[self.at]);
        self.shown_again(ctx);
        self.remember_tabs();
    }

    /// Закрыть вкладку `i`. Последняя закрытая оставляет пустую вкладку.
    pub(super) fn close_tab(&mut self, ctx: &egui::Context, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if i != self.at {
            self.tabs.remove(i);
            if i < self.at {
                self.at -= 1;
            }
        } else {
            self.tab = ProjectTab::default();
            self.tabs.remove(i);
            if self.tabs.is_empty() {
                self.tabs.push(ProjectTab::default());
            }
            self.at = self.at.min(self.tabs.len() - 1);
            std::mem::swap(&mut self.tab, &mut self.tabs[self.at]);
            self.shown_again(ctx);
        }
        self.remember_tabs();
    }

    /// Вкладку открыли заново: заголовок окна и деталь в 3D — её.
    fn shown_again(&mut self, ctx: &egui::Context) {
        let t = texts();
        let title = match &self.tab.project {
            Some(p) => format!("{} — {}", p.title(), t.app_title),
            None => t.app_title.to_string(),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        self.viewport.set_model(None, false);
        self.tab.shown = None;
        self.tab.framed = None;
        if self.tab.part.is_some() && !self.tab.versions.is_empty() {
            self.compare(ctx);
        }
    }

    /// Запомнить вкладки до следующего запуска.
    pub(super) fn remember_tabs(&mut self) {
        let mut roots = Vec::new();
        let mut active = 0;
        for i in 0..self.tabs.len() {
            if let Some(p) = &self.tab_at(i).project {
                if i == self.at {
                    active = roots.len();
                }
                roots.push(p.root().to_path_buf());
            }
        }
        if (&roots, active) != (&self.settings.tabs, self.settings.active) {
            self.settings.tabs = roots;
            self.settings.active = active;
            self.settings.save();
        }
    }

    /// Открыть вкладки прошлого запуска. Пропавшие папки пропускаются.
    pub(super) fn restore_tabs(&mut self, ctx: &egui::Context) {
        let saved = self.settings.tabs.clone();
        let active = self.settings.active;
        let mut opened = None;
        for (i, root) in saved.iter().enumerate() {
            if !root.is_dir() {
                continue;
            }
            match (self.opener)(root) {
                Ok(p) => {
                    self.got_project(ctx, p);
                    if i <= active {
                        opened = Some(self.at);
                    }
                }
                Err(e) => {
                    let t = texts();
                    self.tab.message = Some(format!("{}: {e:#}", t.project_open_failed));
                }
            }
        }
        if let Some(i) = opened {
            self.switch_to(ctx, i);
        }
    }
}

/// Открыт ли в вкладке проект из этой папки.
fn holds(tab: &ProjectTab, root: &Path) -> bool {
    tab.project.as_ref().is_some_and(|p| p.root() == root)
}
