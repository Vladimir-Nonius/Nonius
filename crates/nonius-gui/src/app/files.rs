//! Вкладка «Файлы»: что в версии, что изменилось, разница текста и «Открыть».
//! Изменённые файлы идут первыми. Файл версии открывается копией.

use eframe::egui::{self, Color32, RichText};

use super::jobs::Done;
use super::numbers::size_text;
use super::open_menu::{self, Programs, Target};
use super::{pick_label, App};
use crate::source::{DiffEntry, DiffKind, FileEntry, FileState, OpenWay, Pick};
use crate::texts::texts;

const GREEN: Color32 = Color32::from_rgb(60, 150, 80);
const RED: Color32 = Color32::from_rgb(200, 70, 70);
const YELLOW: Color32 = Color32::from_rgb(190, 140, 20);

/// Файлы, которые ведёт сам Nonius: они под строкой «Служебные файлы».
const SERVICE_FILES: [&str; 3] = ["meta.yaml", "tree.yaml", "preview.png"];

/// Что человек попросил в этом кадре.
enum Ask {
    Select(String),
    Refresh,
}

/// Откуда строим меню правого щелчка.
struct MenuPlace<'a> {
    programs: &'a Programs,
    part: &'a str,
    picks: (Pick, Pick),
}

impl MenuPlace<'_> {
    fn target(&self, pick: Pick, path: &str) -> Target {
        Target {
            part: self.part.to_string(),
            pick,
            path: path.to_string(),
            openable: true,
            model_slot: false,
        }
    }

    fn attach(
        &self,
        response: &egui::Response,
        pick: Pick,
        path: &str,
        asked: &mut Option<open_menu::Ask>,
    ) {
        open_menu::attach(response, self.programs, || self.target(pick, path), asked);
    }
}

impl App {
    /// Прочитать файлы выбранной пары версий.
    pub(super) fn load_files(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(part)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        let picks = self.picks;
        self.jobs.spawn(ctx, move |_| {
            let files = project.files(&part, picks.0, picks.1);
            Done::VersionFiles(part, picks, files)
        });
    }

    /// Пришёл список файлов: изменённые вперёд, выбранный файл — перечитать.
    pub(super) fn got_files(&mut self, ctx: &egui::Context, mut files: Vec<FileEntry>) {
        files.sort_by_key(|f| f.state == FileState::Same);
        let keep = self
            .file
            .take()
            .filter(|p| files.iter().any(|f| f.path == *p));
        self.files = Some((self.picks, files));
        self.file_text = None;
        if let Some(path) = keep {
            self.select_file(ctx, path);
        }
    }

    fn select_file(&mut self, ctx: &egui::Context, path: String) {
        self.file_text = None;
        let text = self.listed_file(&path).is_some_and(|f| f.text);
        self.file = Some(path.clone());
        let (Some(project), Some(part), true) = (self.project.clone(), self.part.clone(), text)
        else {
            return;
        };
        let picks = self.picks;
        self.jobs.spawn(ctx, move |_| {
            let lines = project.file_diff(&part, picks.0, picks.1, &path);
            Done::FileDiff(part, picks, path, lines)
        });
    }

    fn listed_file(&self, path: &str) -> Option<&FileEntry> {
        let (_, files) = self.files.as_ref()?;
        files.iter().find(|f| f.path == path)
    }

    /// Перечитать папку детали: файлы могли появиться, пока окно открыто.
    fn reread(&mut self, ctx: &egui::Context) {
        self.programs.clear();
        if let Some(part) = self.part.clone() {
            self.want = Some((part, Some(self.picks)));
            self.refresh(ctx);
        }
    }
}

/// Нарисовать вкладку.
pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let t = texts();
    let picks = app.picks;
    let Some((_, files)) = app.files.as_ref().filter(|f| f.0 == picks) else {
        ui.spinner();
        return;
    };
    let Some(part) = app.part.as_deref() else {
        return;
    };
    let place = MenuPlace {
        programs: &app.programs,
        part,
        picks,
    };
    let mut ask = None;
    let mut asked = None;
    ui.columns(2, |columns| {
        list(
            &mut columns[0],
            files,
            app.file.as_deref(),
            &mut ask,
            &place,
            &mut asked,
        );
        let selected = app
            .file
            .as_deref()
            .and_then(|p| files.iter().find(|f| f.path == p));
        match selected {
            Some(f) => detail(
                &mut columns[1],
                f,
                app.file_text.as_deref(),
                &place,
                &mut asked,
            ),
            None => {
                columns[1].label(RichText::new(t.choose_file).weak());
            }
        }
    });
    let ctx = ui.ctx().clone();
    match ask {
        Some(Ask::Select(path)) => app.select_file(&ctx, path),
        Some(Ask::Refresh) => app.reread(&ctx),
        None => {}
    }
    app.answer_menu(&ctx, asked);
}

fn list(
    ui: &mut egui::Ui,
    files: &[FileEntry],
    selected: Option<&str>,
    ask: &mut Option<Ask>,
    place: &MenuPlace,
    asked: &mut Option<open_menu::Ask>,
) {
    let t = texts();
    ui.horizontal(|ui| {
        if ui.small_button(t.refresh).clicked() {
            *ask = Some(Ask::Refresh);
        }
    });
    if files.is_empty() {
        ui.label(RichText::new(t.no_files).weak());
    }
    let (service, own): (Vec<&FileEntry>, Vec<&FileEntry>) = files
        .iter()
        .partition(|f| SERVICE_FILES.contains(&f.path.as_str()));
    egui::ScrollArea::vertical()
        .id_salt("file-list")
        .auto_shrink(false)
        .show(ui, |ui| {
            for f in own {
                file_row(ui, f, selected, ask, place, asked);
            }
            if !service.is_empty() {
                egui::CollapsingHeader::new(format!("{} ({})", t.service_files, service.len()))
                    .id_salt("service-files")
                    .show(ui, |ui| {
                        for f in service {
                            file_row(ui, f, selected, ask, place, asked);
                        }
                    });
            }
        });
}

fn file_row(
    ui: &mut egui::Ui,
    f: &FileEntry,
    selected: Option<&str>,
    ask: &mut Option<Ask>,
    place: &MenuPlace,
    asked: &mut Option<open_menu::Ask>,
) {
    let (mark, color) = look(f.state);
    let text = format!("{mark} {}   {}", f.path, sizes(f));
    let mut rich = RichText::new(text);
    if let Some(c) = color {
        rich = rich.color(c);
    }
    let on = selected == Some(f.path.as_str());
    let row = ui.selectable_label(on, rich);
    if row.clicked() && !on {
        *ask = Some(Ask::Select(f.path.clone()));
    }
    // Удалённого файла в «Стало» нет: меню берёт его из «Было».
    let pick = if f.after.is_some() {
        place.picks.1
    } else {
        place.picks.0
    };
    place.attach(&row, pick, &f.path, asked);
}

fn detail(
    ui: &mut egui::Ui,
    f: &FileEntry,
    text: Option<&[DiffEntry]>,
    place: &MenuPlace,
    asked: &mut Option<open_menu::Ask>,
) {
    let t = texts();
    ui.label(RichText::new(&f.path).strong());
    ui.label(format!("{}, {}", state_word(f.state), sizes(f)));
    ui.horizontal(|ui| {
        let mut side = |pick: Pick, has: bool| {
            let label = format!("{} {}", t.open_file, pick_label(pick));
            let hint = match pick {
                Pick::Version(_) => t.open_copy_hint,
                Pick::Current => t.open_real_hint,
            };
            let button = ui
                .add_enabled(has, egui::Button::new(label))
                .on_hover_text(hint);
            if button.clicked() {
                let target = place.target(pick, &f.path);
                *asked = Some(open_menu::Ask::Open(target, OpenWay::Default));
            }
            place.attach(&button, pick, &f.path, asked);
        };
        if place.picks.0 != place.picks.1 {
            side(place.picks.0, f.before.is_some());
        }
        side(place.picks.1, f.after.is_some());
    });
    if !f.text {
        return;
    }
    let Some(lines) = text else {
        ui.spinner();
        return;
    };
    if lines.is_empty() {
        ui.label(RichText::new(t.text_same).weak());
        return;
    }
    ui.separator();
    let height = ui.text_style_height(&egui::TextStyle::Monospace);
    egui::ScrollArea::both()
        .id_salt("file-text")
        .auto_shrink(false)
        .show_rows(ui, height, lines.len(), |ui, rows| {
            for line in &lines[rows] {
                let (mark, color) = match line.kind {
                    DiffKind::Same => (" ", None),
                    DiffKind::Added => ("+", Some(GREEN)),
                    DiffKind::Removed => ("−", Some(RED)),
                    DiffKind::Gap => ("…", None),
                };
                let mut rich = RichText::new(format!("{mark} {}", line.text)).monospace();
                if let Some(c) = color {
                    rich = rich.color(c);
                }
                ui.add(egui::Label::new(rich).extend());
            }
        });
}

fn look(state: FileState) -> (&'static str, Option<Color32>) {
    match state {
        FileState::Same => ("  ", None),
        FileState::Added => ("+", Some(GREEN)),
        FileState::Modified => ("~", Some(YELLOW)),
        FileState::Deleted => ("−", Some(RED)),
    }
}

fn state_word(state: FileState) -> &'static str {
    let t = texts();
    match state {
        FileState::Same => t.file_same,
        FileState::Added => t.file_added,
        FileState::Modified => t.file_modified,
        FileState::Deleted => t.file_deleted,
    }
}

/// Размер: «1,2 МБ» или «1,2 МБ → 1,3 МБ», если он изменился.
fn sizes(f: &FileEntry) -> String {
    match (f.before, f.after) {
        (Some(b), Some(a)) if a != b => format!("{} → {}", size_text(b), size_text(a)),
        (_, Some(n)) | (Some(n), None) => size_text(n),
        (None, None) => String::new(),
    }
}
