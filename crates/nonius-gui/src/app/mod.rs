//! Окно целиком: состояние, порядок работы и раскладка панелей.
//!
//! Порядок: открыть проект → список деталей → выбрать деталь → её версии →
//! сравнить выбранную пару → показать в 3D. Всё долгое — в фоне (`jobs.rs`).

mod jobs;
mod numbers;
mod panels;
mod settings;
mod start;
mod toolbar;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;

use crate::source::{
    default_picks, Comparison, Opener, PartEntry, Pick, ProjectSource, VersionEntry,
};
use crate::texts::texts;
use crate::view::{Look, Mode, Model, Viewport, ViewportEvent};
use jobs::{Done, Jobs};
pub use settings::Settings;

/// С чем открыть окно.
pub struct Options {
    /// Открытый проект; `None` — начальный экран (или последний проект).
    pub project: Option<Box<dyn ProjectSource>>,
    /// Как открыть проект по папке.
    pub opener: Opener,
    /// Сразу выбрать деталь.
    pub part: Option<String>,
    /// И сразу сравнить эти версии.
    pub picks: Option<(Pick, Pick)>,
}

/// Состояние окна.
pub struct App {
    opener: Opener,
    project: Option<Arc<dyn ProjectSource>>,
    parts: Vec<PartEntry>,
    part: Option<String>,
    versions: Vec<VersionEntry>,
    picks: (Pick, Pick),
    want: Option<(String, Option<(Pick, Pick)>)>,
    comparison: Option<Comparison>,
    shown: Option<(String, (Pick, Pick))>,
    viewport: Viewport,
    mode: Mode,
    look: Look,
    settings: Settings,
    selected: Option<usize>,
    jobs: Jobs,
    message: Option<String>,
    path_input: String,
}

impl App {
    /// Окно на устройстве `render` (без видеокарты — без 3D).
    pub fn new(
        ctx: &egui::Context,
        render: Option<eframe::egui_wgpu::RenderState>,
        options: Options,
        settings: Settings,
    ) -> Self {
        let look = Look {
            xray: settings.xray,
            edges: settings.edges,
            ..Look::default()
        };
        let mut app = Self {
            opener: options.opener,
            project: None,
            parts: Vec::new(),
            part: None,
            versions: Vec::new(),
            picks: (Pick::Current, Pick::Current),
            want: options.part.map(|p| (p, options.picks)),
            comparison: None,
            shown: None,
            viewport: Viewport::new(render),
            mode: Mode::Changes,
            look,
            selected: None,
            jobs: Jobs::default(),
            message: None,
            path_input: String::new(),
            settings,
        };
        match options.project {
            Some(p) => app.set_project(ctx, p),
            None => {
                if let Some(last) = app.settings.last_project.clone().filter(|p| p.is_dir()) {
                    app.open(ctx, last);
                }
            }
        }
        app
    }

    /// Открыть проект по папке (в фоне).
    fn open(&mut self, ctx: &egui::Context, path: PathBuf) {
        let opener = self.opener.clone();
        self.message = None;
        self.jobs.spawn(ctx, move |_| Done::Opened(opener(&path)));
    }

    fn set_project(&mut self, ctx: &egui::Context, project: Box<dyn ProjectSource>) {
        self.settings.last_project = Some(project.root().to_path_buf());
        self.settings.save();
        let project: Arc<dyn ProjectSource> = Arc::from(project);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{} — {}",
            project.title(),
            texts().app_title
        )));
        self.project = Some(project.clone());
        self.parts.clear();
        self.select(None);
        self.jobs.spawn(ctx, move |_| Done::Parts(project.parts()));
    }

    /// Выбрать деталь: прочитать её версии, потом сравнить.
    fn select_part(&mut self, ctx: &egui::Context, name: &str) {
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
        self.part = part;
        self.versions.clear();
        self.comparison = None;
        self.selected = None;
        self.message = None;
    }

    /// Сравнить выбранную пару версий.
    fn compare(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(part)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        let picks = self.picks;
        self.jobs.spawn(ctx, move |progress| {
            let result = project.compare(&part, picks.0, picks.1, progress);
            Done::Compared(part, picks, result)
        });
    }

    fn take_results(&mut self, ctx: &egui::Context) {
        let t = texts();
        for done in self.jobs.finished() {
            match done {
                Done::Opened(Ok(p)) => self.set_project(ctx, p),
                Done::Opened(Err(e)) => {
                    self.message = Some(format!("{}: {e:#}", t.project_open_failed))
                }
                Done::Parts(Ok(parts)) => {
                    self.parts = parts;
                    let want = self.want.as_ref().map(|w| w.0.clone());
                    let only = (self.parts.len() == 1).then(|| self.parts[0].name.clone());
                    if let Some(name) = want.or(only) {
                        if self.parts.iter().any(|p| p.name == name) {
                            self.select_part(ctx, &name);
                        }
                    }
                }
                Done::Versions(name, Ok(v)) if self.part.as_deref() == Some(&name) => {
                    let entry = self.parts.iter().find(|p| p.name == name);
                    let defaults = entry.map(|e| default_picks(e, &v));
                    let wanted = match self.want.take() {
                        Some((n, picks)) if n == name => picks,
                        _ => None,
                    };
                    self.versions = v;
                    self.picks = wanted.or(defaults).unwrap_or(self.picks);
                    self.compare(ctx);
                }
                Done::Compared(name, picks, Ok(c))
                    if self.part.as_deref() == Some(&name) && self.picks == picks =>
                {
                    let same_part = self.shown.as_ref().is_some_and(|s| s.0 == name);
                    let model = c.scene.as_ref().and_then(Model::new);
                    self.viewport.set_model(model, same_part);
                    self.shown = Some((name, picks));
                    self.comparison = Some(c);
                    self.selected = None;
                }
                Done::Parts(Err(e)) | Done::Versions(_, Err(e)) | Done::Compared(_, _, Err(e)) => {
                    self.message = Some(format!("{}: {e:#}", t.error))
                }
                Done::Crashed => self.message = Some(t.crashed.to_string()),
                Done::Versions(..) | Done::Compared(..) => {} // устарело: выбрали другое
            }
        }
    }

    /// Человек поменял переключатель — запомнить.
    fn remember_look(&mut self) {
        let s = &mut self.settings;
        if (s.xray, s.edges) != (self.look.xray, self.look.edges) {
            s.xray = self.look.xray;
            s.edges = self.look.edges;
            s.save();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.take_results(ctx);
        if let Some(dir) = dropped_folder(ctx) {
            self.open(ctx, dir);
        }

        if self.project.is_none() {
            start::show(self, ctx);
            return;
        }
        panels::status_bar(self, ctx);
        panels::parts(self, ctx);
        panels::changes(self, ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                toolbar::show(self, ui);
                let event = self.viewport.show(
                    ui,
                    self.mode,
                    &self.look,
                    self.selected,
                    self.settings.wheel_reverse,
                );
                if let Some(ViewportEvent::Picked(place)) = event {
                    self.selected = place;
                }
                panels::overlay(self, ui);
            });
        self.remember_look();
    }
}

/// Папка, которую перетащили в окно (файл — его папка).
fn dropped_folder(ctx: &egui::Context) -> Option<PathBuf> {
    ctx.input(|i| {
        i.raw.dropped_files.iter().find_map(|f| {
            let p = f.path.as_deref()?;
            Some(if p.is_dir() { p } else { p.parent()? }.to_path_buf())
        })
    })
}

/// Подпись версии: `v3` или «текущее».
fn pick_label(pick: Pick) -> String {
    match pick {
        Pick::Version(n) => format!("v{n}"),
        Pick::Current => texts().current.to_string(),
    }
}

/// Имя картинки по умолчанию: «кронштейн v2-v3.png».
fn picture_name(part: &str, picks: (Pick, Pick)) -> String {
    let clean = |s: String| s.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_");
    clean(format!(
        "{part} {}-{}.png",
        pick_label(picks.0),
        pick_label(picks.1)
    ))
}

/// Есть ли папка проекта по этому пути (для поля ввода пути).
fn looks_like_dir(path: &str) -> bool {
    !path.trim().is_empty() && Path::new(path.trim()).is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picture_names_are_safe() {
        assert_eq!(
            picture_name("корпус/крышка", (Pick::Version(2), Pick::Current)),
            "корпус_крышка v2-текущее.png"
        );
        assert!(!looks_like_dir("   "));
    }
}
