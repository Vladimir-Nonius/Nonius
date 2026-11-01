//! Окно целиком: состояние, порядок работы и раскладка панелей.
//!
//! Порядок: открыть проект → список деталей → выбрать деталь → её версии →
//! сравнить выбранную пару → показать в 3D. Действия — `actions.rs`, их
//! результаты — `results.rs`, всё долгое — в фоне (`jobs.rs`).

mod actions;
mod files;
mod history;
mod jobs;
mod material;
mod numbers;
mod panels;
mod results;
mod save;
mod settings;
mod start;
mod tabs;
mod toolbar;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;

use crate::source::{
    Comparison, DiffEntry, FileEntry, MassEntry, MaterialEntry, Opener, PartEntry, Pick,
    ProjectSource, VersionEntry,
};
use crate::texts::texts;
use crate::view::{Look, Mode, Viewport, ViewportEvent};
use jobs::Jobs;
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
    /// Ошибка — показывается в строке состояния, пока не выбрали другое.
    message: Option<String>,
    /// Что получилось: «Сохранена версия …», «Сравнение отменено».
    notice: Option<String>,
    path_input: String,
    materials: Vec<MaterialEntry>,
    /// Материал и масса для пары версий: «Было», «Стало», сейчас.
    masses: Option<((Pick, Pick), [MassEntry; 3])>,
    /// Открытое окошко «Сохранить версию».
    save: Option<save::SaveDialog>,
    /// Ввод своего материала.
    custom: material::Custom,
    /// Материал записывается — список выбора ждёт.
    material_busy: bool,
    /// Открытая вкладка под 3D; `None` — панель свёрнута.
    tab: Option<tabs::Tab>,
    /// Файлы для пары версий.
    files: Option<((Pick, Pick), Vec<FileEntry>)>,
    /// Выбранный файл на вкладке «Файлы».
    file: Option<String>,
    /// Строки выбранного текстового файла.
    file_text: Option<Vec<DiffEntry>>,
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
            notice: None,
            path_input: String::new(),
            materials: Vec::new(),
            masses: None,
            save: None,
            custom: material::Custom::default(),
            material_busy: false,
            tab: Some(tabs::Tab::Files),
            files: None,
            file: None,
            file_text: None,
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
        tabs::show(self, ctx);
        save::show(self, ctx);
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
