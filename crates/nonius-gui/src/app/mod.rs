//! Окно целиком: состояние, порядок работы и раскладка панелей.
//!
//! Порядок: открыть проект → список деталей → выбрать деталь → её версии →
//! сравнить выбранную пару → показать в 3D. Действия — `actions.rs`, их
//! результаты — `results.rs`, всё долгое — в фоне (`jobs.rs`).

mod actions;
mod add_model;
mod choose;
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
mod new_part;
mod numbers;
mod open_menu;
mod panels;
mod part_actions;
mod part_dialogs;
mod results;
mod save;
mod settings;
mod start;
mod tabs;
mod toolbar;
mod tree;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;

use crate::source::{
    Chooser, Comparison, DiffEntry, FileEntry, Machine, MassEntry, MaterialEntry, Opener,
    PartEntry, Pick, ProjectSource, VersionEntry,
};
use crate::texts::texts;
use crate::view::{Look, Mode, Viewport, ViewportEvent};
pub use choose::SystemChooser;
use jobs::Jobs;
pub use settings::Settings;

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
}

/// Состояние окна.
pub struct App {
    opener: Opener,
    creator: Opener,
    chooser: Arc<dyn Chooser>,
    project: Option<Arc<dyn ProjectSource>>,
    parts: Vec<PartEntry>,
    programs: open_menu::Programs,
    part: Option<String>,
    versions: Vec<VersionEntry>,
    picks: (Pick, Pick),
    want: Option<(String, Option<(Pick, Pick)>)>,
    comparison: Option<Comparison>,
    shown: Option<(String, (Pick, Pick))>,
    /// Деталь, под которую настроена камера: её вид не сбрасывается.
    framed: Option<String>,
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
    /// Что сказать, когда деталь будет показана.
    sticky: Option<String>,
    path_input: String,
    /// Строка поиска в дереве.
    search: String,
    /// Раскрытые в дереве детали.
    expanded: BTreeSet<String>,
    /// Версии раскрытых деталей, кроме выбранной.
    listed: HashMap<String, Vec<VersionEntry>>,
    /// С чем сравнивается деталь вне истории: имя файла или детали.
    outside: Option<String>,
    /// Открытое окошко действия над деталью.
    dialog: Option<part_dialogs::Dialog>,
    materials: Vec<MaterialEntry>,
    /// Материал и масса для пары версий: «Было», «Стало», сейчас.
    masses: Option<((Pick, Pick), [MassEntry; 3])>,
    /// Открытое окошко «Сохранить версию».
    save: Option<save::SaveDialog>,
    /// Открытое окошко «Новая деталь».
    new_part: Option<new_part::NewPart>,
    /// Открытое окошко «Добавить модель».
    add: Option<add_model::AddModel>,
    /// Модели бросили в пустое окно: создать проект в этой папке?
    new_here: Option<(PathBuf, Vec<PathBuf>)>,
    /// Что добавить, когда откроется новый проект.
    after_open: Vec<PathBuf>,
    /// Окошко первого запуска.
    first_run: Option<first_run::FirstRun>,
    /// Git есть, а Git LFS нет.
    no_lfs: bool,
    /// Окно было в фокусе в прошлом кадре.
    was_focused: bool,
    /// Вернулись в окно: перечитать детали, как только оно освободится.
    reread_wanted: bool,
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
        let ready = options.machine.readiness();
        let mut app = Self {
            opener: options.opener,
            creator: options.creator,
            chooser: options.chooser,
            project: None,
            parts: Vec::new(),
            programs: Default::default(),
            part: None,
            versions: Vec::new(),
            picks: (Pick::Current, Pick::Current),
            want: None,
            comparison: None,
            shown: None,
            framed: None,
            viewport: Viewport::new(render),
            mode: Mode::Changes,
            look,
            selected: None,
            jobs: Jobs::default(),
            message: None,
            notice: None,
            sticky: None,
            path_input: String::new(),
            search: String::new(),
            expanded: BTreeSet::new(),
            listed: HashMap::new(),
            outside: None,
            dialog: None,
            materials: Vec::new(),
            masses: None,
            save: None,
            new_part: None,
            add: None,
            new_here: None,
            after_open: Vec::new(),
            first_run: first_run::FirstRun::check(options.machine, ready),
            no_lfs: ready.git && !ready.lfs,
            was_focused: true,
            reread_wanted: false,
            custom: material::Custom::default(),
            material_busy: false,
            tab: None,
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
        app.want = options.part.map(|p| (p, options.picks));
        app
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.take_results(ctx);
        self.take_dropped(ctx);
        self.notice_focus(ctx);
        first_run::show(self, ctx);

        if self.project.is_none() {
            start::show(self, ctx);
            return;
        }
        file_menu::show(self, ctx);
        panels::error_bar(self, ctx);
        panels::status_bar(self, ctx);
        map::show(self, ctx);
        tree::show(self, ctx);
        panels::changes(self, ctx);
        tabs::show(self, ctx);
        save::show(self, ctx);
        new_part::show(self, ctx);
        add_model::show(self, ctx);
        part_dialogs::show(self, ctx);
        // Заливка нужна: зазор между полосой и 3D иначе показывает чёрный фон.
        let fill = ctx.style().visuals.panel_fill;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(fill))
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
