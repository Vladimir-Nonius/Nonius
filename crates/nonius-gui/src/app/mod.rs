//! Окно целиком: состояние, порядок работы и раскладка панелей.
//!
//! Порядок: открыть проект → список деталей → выбрать деталь → её версии →
//! сравнить выбранную пару → показать в 3D. Действия — `actions.rs`, их
//! результаты — `results.rs`, всё долгое — в фоне (`jobs.rs`).

mod actions;
mod add_model;
mod bottom;
mod choose;
mod draft;
mod drag_hint;
mod drop_zones;
mod dropped;
mod file_menu;
mod files;
mod first_run;
mod history;
mod jobs;
mod legend;
mod map;
mod material;
mod model_names;
mod new_folder;
mod new_part;
mod node;
mod numbers;
mod open_menu;
mod panels;
mod part_actions;
mod part_dialogs;
mod project_tab;
mod results;
mod save;
mod settings;
mod settings_window;
mod shown;
mod start;
mod strip;
mod structure;
mod toolbar;
mod tree;
mod tree_folders;
mod upgrade;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;

use crate::source::{Chooser, Discard, Machine, Mover, Opener, Pick, ProjectSource};
use crate::texts::texts;
use crate::view::{Look, Mode, Viewport, ViewportEvent};
pub use choose::SystemChooser;
use project_tab::ProjectTab;
pub use settings::{drafts_dir, Settings};

/// С чем открыть окно.
pub struct Options {
    /// Открытый проект; `None` — начальный экран (или последний проект).
    pub project: Option<Box<dyn ProjectSource>>,
    /// Как открыть проект по папке.
    pub opener: Opener,
    /// Как создать проект в папке и открыть его.
    pub creator: Opener,
    /// Компьютер: есть ли git и имя автора.
    pub machine: Arc<dyn Machine>,
    /// Окна выбора файла и папки.
    pub chooser: Arc<dyn Chooser>,
    /// Сразу выбрать деталь.
    pub part: Option<String>,
    /// И сразу сравнить эти версии.
    pub picks: Option<(Pick, Pick)>,
    /// Папка черновиков; `None` — без черновиков, начальный экран.
    pub drafts: Option<PathBuf>,
    /// Как перенести черновик в настоящую папку.
    pub mover: Mover,
    /// Как убрать черновик, который решили не сохранять.
    pub discard: Discard,
}

/// Состояние окна.
pub struct App {
    opener: Opener,
    creator: Opener,
    chooser: Arc<dyn Chooser>,
    /// Открытая вкладка проекта.
    tab: ProjectTab,
    /// Все вкладки по порядку; на месте открытой (`at`) — пустышка.
    tabs: Vec<ProjectTab>,
    at: usize,
    viewport: Viewport,
    mode: Mode,
    look: Look,
    settings: Settings,
    path_input: String,
    /// Модели бросили в пустое окно: создать проект в этой папке?
    new_here: Option<(PathBuf, Vec<PathBuf>)>,
    /// Окошко первого запуска.
    first_run: Option<first_run::FirstRun>,
    /// Git есть, а Git LFS нет.
    no_lfs: bool,
    /// Окно было в фокусе в прошлом кадре.
    was_focused: bool,
    /// Окно поднято поверх всех, пока над ним несут файл из Проводника.
    raised: bool,
    /// Папка черновиков.
    drafts: Option<PathBuf>,
    mover: Mover,
    discard: Discard,
    /// Создать черновик, как только окно будет готово.
    draft_wanted: bool,
    /// Окошко «Проект не сохранён».
    closing: Option<draft::Closing>,
    /// Пользователь согласился потерять черновики: окно закрывается.
    may_close: bool,
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
        let ready = options.machine.readiness();
        let mut app = Self {
            opener: options.opener,
            creator: options.creator,
            chooser: options.chooser,
            tab: ProjectTab::default(),
            tabs: vec![ProjectTab::default()],
            at: 0,
            viewport: Viewport::new(render),
            mode: Mode::Changes,
            look,
            path_input: String::new(),
            new_here: None,
            first_run: first_run::FirstRun::check(options.machine, ready),
            no_lfs: ready.git && !ready.lfs,
            was_focused: true,
            raised: false,
            drafts: options.drafts,
            mover: options.mover,
            discard: options.discard,
            draft_wanted: false,
            closing: None,
            may_close: false,
            settings,
        };
        app.restore_tabs(ctx);
        if let Some(p) = options.project {
            app.got_project(ctx, p);
        }
        app.draft_wanted = app.drafts.is_some() && app.tab.project.is_none();
        app.tab.want = options.part.map(|p| (p, options.picks));
        app
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.take_results(ctx);
        self.take_dropped(ctx);
        self.notice_focus(ctx);
        first_run::show(self, ctx);
        self.watch_window_close(ctx);
        if self.draft_wanted && self.first_run.is_none() && !self.tab.jobs.busy() {
            self.draft_wanted = false;
            self.new_project(ctx);
        }
        // Меню выше вкладок: оно у окна одно на все проекты.
        if self.tab.project.is_some() {
            file_menu::show(self, ctx);
        }
        self.move_when_free(ctx);
        self.reopen_when_free(ctx);
        strip::show(self, ctx);
        draft::bar(self, ctx);
        upgrade::bar(self, ctx);
        draft::ask(self, ctx);
        upgrade::ask(self, ctx);

        if self.tab.project.is_none() {
            start::show(self, ctx);
            return;
        }
        panels::error_bar(self, ctx);
        panels::status_bar(self, ctx);
        map::show(self, ctx);
        // Строки дерева и «Структуры» отметят свои места заново.
        self.tab.on_screen.clear();
        tree::show(self, ctx);
        panels::changes(self, ctx);
        bottom::show(self, ctx);
        save::show(self, ctx);
        new_part::show(self, ctx);
        new_folder::show(self, ctx);
        settings_window::show(self, ctx);
        add_model::show(self, ctx);
        part_dialogs::show(self, ctx);
        // Заливка нужна: зазор между полосой и 3D иначе показывает чёрный фон.
        let fill = ctx.style().visuals.panel_fill;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(fill))
            .show(ctx, |ui| {
                structure::switch(self, ui);
                if self.tab.view == structure::View::Structure {
                    structure::show(self, ui);
                    return;
                }
                toolbar::show(self, ui);
                let event = self.viewport.show(
                    ui,
                    self.mode,
                    &self.look,
                    self.tab.selected,
                    self.settings.wheel_reverse,
                );
                if let Some(ViewportEvent::Picked(place)) = event {
                    self.tab.selected = place;
                }
                panels::overlay(self, ui);
            });
        self.show_drop_target(ctx);
        self.show_drag_ghost(ctx);
        self.remember_look();
    }
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
