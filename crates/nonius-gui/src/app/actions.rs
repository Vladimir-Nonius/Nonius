//! Что окно делает по просьбе человека: открыть проект, выбрать деталь и
//! версии, сравнить, сохранить версию, назначить материал. Долгое — в фоне.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

use super::jobs::Done;
use super::App;
use crate::source::{Pick, ProjectSource};
use crate::texts::texts;

impl App {
    /// Открыть проект по папке (в фоне).
    pub(super) fn open(&mut self, ctx: &egui::Context, path: PathBuf) {
        let opener = self.opener.clone();
        self.message = None;
        self.jobs.spawn(ctx, move |_| Done::Opened(opener(&path)));
    }

    pub(super) fn set_project(&mut self, ctx: &egui::Context, project: Box<dyn ProjectSource>) {
        self.settings.last_project = Some(project.root().to_path_buf());
        self.settings.save();
        let project: Arc<dyn ProjectSource> = Arc::from(project);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{} — {}",
            project.title(),
            texts().app_title
        )));
        self.materials = project.materials();
        self.project = Some(project.clone());
        self.parts.clear();
        self.select(None);
        self.jobs.spawn(ctx, move |_| Done::Parts(project.parts()));
    }

    /// Выбрать деталь: прочитать её версии, потом сравнить.
    pub(super) fn select_part(&mut self, ctx: &egui::Context, name: &str) {
        let Some(project) = self.project.clone() else {
            return;
        };
        self.select(Some(name.to_string()));
        let name = name.to_string();
        self.jobs.spawn(ctx, move |_| {
            let v = project.versions(&name);
            Done::Versions(name, v)
        });
    }

    fn select(&mut self, part: Option<String>) {
        self.jobs.cancel();
        self.part = part;
        self.versions.clear();
        self.comparison = None;
        self.masses = None;
        self.selected = None;
        self.message = None;
        self.notice = None;
        self.save = None;
    }

    /// Перечитать детали и версии выбранной детали после сохранения или
    /// смены материала. Показ остаётся, пока не придёт новое сравнение.
    pub(super) fn refresh(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(name)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        self.jobs.spawn(ctx, move |_| match project.parts() {
            Ok(parts) => {
                let versions = project.versions(&name);
                Done::Refreshed(parts, name, versions)
            }
            Err(e) => Done::Parts(Err(e)),
        });
    }

    /// Показать пару версий: сравнить её и прочитать массы.
    pub(super) fn show_picks(&mut self, ctx: &egui::Context, picks: (Pick, Pick)) {
        self.picks = picks;
        self.compare(ctx);
    }

    /// Сравнить выбранную пару версий. Прошлое сравнение, если ещё идёт,
    /// отменяется: его результат уже не нужен.
    pub(super) fn compare(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(part)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        let picks = self.picks;
        self.notice = None;
        let (p, name) = (project, part);
        self.jobs.spawn_cancellable(ctx, move |progress| {
            let result = p.compare(&name, picks.0, picks.1, progress);
            Done::Compared(name, picks, result)
        });
        self.load_masses(ctx);
    }

    /// Прочитать материал и массу для выбранной пары версий и сейчас.
    pub(super) fn load_masses(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(part)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        let picks = self.picks;
        self.jobs.spawn(ctx, move |_| {
            let mass = |pick| project.mass(&part, pick);
            let result = (|| Ok([mass(picks.0)?, mass(picks.1)?, mass(Pick::Current)?]))();
            Done::Masses(part, picks, result)
        });
    }

    /// «Отменить» в строке состояния: бросить сравнение и вернуть пару
    /// версий, которая сейчас на экране.
    pub(super) fn cancel_compare(&mut self) {
        if !self.jobs.cancel() {
            return;
        }
        if let Some((name, picks)) = &self.shown {
            if self.part.as_ref() == Some(name) {
                self.picks = *picks;
            }
        }
        self.notice = Some(texts().compare_cancelled.to_string());
    }

    /// Открыть окошко «Сохранить версию» и прочитать изменённые файлы.
    pub(super) fn start_save(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(part)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        let latest = self.versions.first().map(|v| v.number);
        let next = latest.map_or(1, |n| n + 1);
        self.save = Some(super::save::SaveDialog::new(&part, next));
        // Показать в 3D именно то, что попадёт в новую версию.
        if let Some(n) = latest {
            let picks = (Pick::Version(n), Pick::Current);
            if self.picks != picks {
                self.show_picks(ctx, picks);
            }
        }
        self.jobs.spawn(ctx, move |_| {
            let files = project.changed_files(&part);
            Done::Files(part, files)
        });
    }

    /// Сохранить версию с описанием из окошка.
    pub(super) fn save_version(&mut self, ctx: &egui::Context) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let Some(dialog) = self.save.as_mut() else {
            return;
        };
        dialog.saving = true;
        dialog.error = None;
        let (part, message) = (dialog.part.clone(), dialog.message.clone());
        self.jobs.spawn(ctx, move |_| {
            let saved = project.save_version(&part, &message);
            Done::Saved(part, saved)
        });
    }

    /// Назначить материал детали сейчас. `None` — убрать.
    pub(super) fn set_material(
        &mut self,
        ctx: &egui::Context,
        material: Option<(String, Option<f64>)>,
    ) {
        let (Some(project), Some(part)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        self.jobs.spawn(ctx, move |_| {
            let set = match &material {
                Some((name, density)) => project.set_material(&part, Some(name), *density),
                None => project.set_material(&part, None, None),
            };
            Done::MaterialSet(part, set)
        });
    }

    /// Человек поменял переключатель — запомнить.
    pub(super) fn remember_look(&mut self) {
        let s = &mut self.settings;
        if (s.xray, s.edges) != (self.look.xray, self.look.edges) {
            s.xray = self.look.xray;
            s.edges = self.look.edges;
            s.save();
        }
    }
}
