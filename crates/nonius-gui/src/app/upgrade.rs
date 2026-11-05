//! Проект старого формата: детали в `parts/` и `assemblies/`. Его можно
//! смотреть, но не менять. При открытии окно один раз спрашивает, обновить
//! ли его; потом над проектом остаётся полоса с той же кнопкой. Обновление —
//! одна запись истории, после него проект открывается заново. Так же заново
//! открывается проект после «Настроек».

use eframe::egui::{self, RichText};

use super::choice;
use super::jobs::Done;
use super::project_tab::ProjectTab;
use super::App;
use crate::texts::texts;

impl App {
    /// Обновить проект или просто открыть его заново (после настроек), как
    /// только вкладка свободна: фоновые работы ещё могут читать файлы,
    /// которые переедут. Выбранная деталь и вид остаются.
    pub(super) fn reopen_when_free(&mut self, ctx: &egui::Context) {
        let free = !self.tab.jobs.busy() && self.tab.jobs.all_finished();
        if !(self.tab.upgrading || self.tab.reopen) || !free {
            return;
        }
        let Some(project) = self.tab.project.take() else {
            return;
        };
        let upgrade = self.tab.upgrading;
        let (part, view) = (self.tab.part.take(), self.tab.view);
        self.tab = ProjectTab::default();
        self.tab.want = part.map(|p| (p, None));
        self.tab.view = view;
        let opener = self.opener.clone();
        self.tab.jobs.spawn(ctx, move |_| {
            let root = project.root().to_path_buf();
            let error = if upgrade {
                project.upgrade().err().map(|e| format!("{e:#}"))
            } else {
                None
            };
            // Проект открывается заново: прежний держит старые пути.
            drop(project);
            Done::Moved(opener(&root), error)
        });
    }
}

/// Цвета жёлтой полосы: светлая и тёмная тема.
pub(super) fn bar_colors(ctx: &egui::Context) -> (egui::Color32, egui::Color32) {
    if ctx.style().visuals.dark_mode {
        (
            egui::Color32::from_rgb(74, 60, 20),
            egui::Color32::from_rgb(250, 220, 140),
        )
    } else {
        (
            egui::Color32::from_rgb(255, 240, 190),
            egui::Color32::from_rgb(110, 75, 0),
        )
    }
}

/// Полоса над проектом старого формата.
pub fn bar(app: &mut App, ctx: &egui::Context) {
    let old = app.tab.project.as_ref().is_some_and(|p| p.old_layout());
    if !old && !app.tab.upgrading {
        return;
    }
    let t = texts();
    let (fill, text) = bar_colors(ctx);
    egui::TopBottomPanel::top("old-layout-bar")
        .frame(egui::Frame::side_top_panel(&ctx.style()).fill(fill))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(t.old_layout_bar).color(text));
                if app.tab.upgrading {
                    ui.spinner();
                    ui.label(RichText::new(t.upgrading).color(text));
                } else if ui.button(t.upgrade).clicked() {
                    app.tab.upgrading = true;
                }
            });
        });
}

/// Окошко «Проект старого формата» — один раз при открытии.
pub fn ask(app: &mut App, ctx: &egui::Context) {
    if !app.tab.ask_upgrade {
        return;
    }
    let t = texts();
    let mut answer = None;
    egui::Modal::new(egui::Id::new("old-layout")).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.heading(t.old_layout);
        ui.add_space(6.0);
        ui.label(t.old_layout_text);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let later = ui.button(t.not_now);
            let upgrade = ui.button(t.upgrade);
            if later.clicked() {
                answer = Some(false);
            }
            if upgrade.clicked() {
                answer = Some(true);
            }
            choice::keep_focus(&[later, upgrade], 1);
        });
    });
    if let Some(yes) = answer {
        app.tab.ask_upgrade = false;
        app.tab.upgrading = yes;
    }
}
