//! Дерево слева: поиск, папки и детали проекта, внутри раскрытых деталей —
//! их история. Папки и перетаскивание — в `tree_folders.rs`.

use eframe::egui::{self, Color32, RichText};

use super::jobs::Done;
use super::marquee;
use super::node::Node;
use super::open_menu::{self, Ask, Target};
use super::selection;
use super::shown::Shown;
use super::tree_folders;
use super::App;
use crate::source::{Listing, PartEntry, Pick};
use crate::texts::texts;

/// Паспорт детали: он есть всегда, даже без файла модели.
const META_FILE: &str = nonius_core::project::META_FILE;

/// Что человек сделал в дереве за этот кадр. Исполняется после рисования:
/// пока дерево рисуется, окно занято им.
#[derive(Default)]
pub(super) struct Acts {
    /// Щёлкнули по предмету (`true` — правой кнопкой).
    pub picked: Option<(Node, bool)>,
    pub unfolded: Option<String>,
    pub asked: Option<Ask>,
    /// «Свернуть всё».
    pub fold_all: bool,
    /// «Новая деталь здесь…» в этой папке.
    pub new_part: Option<String>,
    /// «Новая папка…» внутри этой.
    pub new_folder: Option<String>,
    /// Что где на экране: строки папок, деталей и файлов.
    pub shown: Vec<Shown>,
    /// Открыть обычный файл папки (`true` — показать в папке).
    pub open_file: Option<(String, bool)>,
    /// Рамка мышью по пустому месту.
    pub sweep: marquee::Sweep,
}

/// Нарисовать дерево.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let mut acts = Acts::default();
    egui::SidePanel::left("parts")
        .default_width(250.0)
        .show(ctx, |ui| {
            if let Some(p) = &app.tab.project {
                ui.add_space(6.0);
                ui.heading(p.title());
            }
            let search = egui::TextEdit::singleline(&mut app.tab.search)
                .hint_text(t.search_parts)
                .desired_width(f32::INFINITY);
            ui.add(search);
            tree_folders::buttons(app, ui, &mut acts);
            ui.separator();
            if app.tab.parts.is_empty() && app.tab.folders.is_empty() && !app.tab.jobs.busy() {
                ui.label(RichText::new(t.no_parts).weak());
            }
            let searching = !app.tab.search.trim().is_empty();
            let found: Vec<PartEntry> = app
                .tab
                .parts
                .iter()
                .filter(|p| matches(&p.name, &app.tab.search))
                .cloned()
                .collect();
            if found.is_empty() && searching {
                ui.label(RichText::new(t.nothing_found).weak());
            }
            let files: Vec<String> = app
                .tab
                .loose
                .iter()
                .filter(|f| matches(tree_folders::last(f), &app.tab.search))
                .cloned()
                .collect();
            let folders = tree_folders::visible(&app.tab.folders, &found, searching);
            egui::ScrollArea::vertical()
                .id_salt("tree")
                .auto_shrink(false)
                // Тянуть по пустому месту — рамка, а не прокрутка.
                .scroll_source(egui::scroll_area::ScrollSource {
                    drag: false,
                    ..Default::default()
                })
                .show(ui, |ui| {
                    // Пустое место — раньше строк: строки лягут поверх.
                    let bg = marquee::background(ui, "tree-bg");
                    let tree = tree_folders::Tree {
                        folders: &folders,
                        parts: &found,
                        files: &files,
                        searching,
                    };
                    tree_folders::dir(app, ui, &tree, "", &mut acts);
                    tree_folders::root_drop(ui, &mut acts);
                    // Весь проект — корень: брошенное на деталь в корне
                    // подсвечивает всё дерево.
                    let all = ui.min_rect();
                    acts.shown
                        .push(Shown::on(ui, all, Node::Folder(String::new())));
                    acts.sweep = marquee::track(ui, &bg);
                });
        });
    tree_folders::apply(app, ctx, &mut acts);
    if let Some(name) = acts.unfolded {
        app.list_versions(ctx, &name);
    }
    app.answer_menu(ctx, acts.asked);
}

/// Строка детали со стрелкой, а под раскрытой — её история.
pub(super) fn part_line(app: &mut App, ui: &mut egui::Ui, p: &PartEntry, acts: &mut Acts) {
    let node = Node::Part(p.name.clone());
    // Открытая в 3D и выделенная — не одно и то же: Ctrl выделяет, не
    // открывая.
    let opened = app.tab.part.as_ref() == Some(&p.name);
    let chosen = app.tab.selection.contains(&node);
    let open = app.tab.expanded.contains(&p.name);
    ui.horizontal(|ui| {
        let arrow = if open { "▾" } else { "▸" };
        let toggle = ui.add(egui::Button::new(arrow).frame(false)).clicked();
        let r = tree_folders::drag_part(part_row(ui, p, chosen), p);
        if opened && !chosen {
            selection::opened_frame(ui, r.rect);
        }
        acts.shown.push(Shown::on(ui, r.rect, node.clone()));
        if toggle && open {
            app.tab.expanded.remove(&p.name);
        } else if toggle {
            app.tab.expanded.insert(p.name.clone());
            if !opened {
                acts.unfolded = Some(p.name.clone());
            }
        } else if r.clicked() || r.secondary_clicked() {
            // Правый щелчок тоже выделяет: видно, к чему меню.
            acts.picked = Some((node, r.secondary_clicked()));
        }
        open_menu::attach(&r, &app.tab.programs, || target(p), &mut acts.asked);
    });
    if open {
        ui.indent(&p.name, |ui| super::history::show(app, ui, p));
    }
}

impl App {
    /// Раскрыть деталь в дереве, не выбирая её.
    pub(super) fn list_versions(&mut self, ctx: &egui::Context, name: &str) {
        let Some(project) = self.tab.project.clone() else {
            return;
        };
        let name = name.to_string();
        self.tab.jobs.spawn(ctx, move |_| {
            let v = project.versions(&name);
            Done::Listed(name, v)
        });
    }

    /// Новый список деталей и папок: раскрытые, что изменились, перечитать.
    pub(super) fn set_parts(&mut self, ctx: &egui::Context, listing: Listing) {
        let old = std::mem::replace(&mut self.tab.parts, listing.parts);
        self.tab.folders = listing.folders;
        self.tab.loose = listing.files;
        let names = self.tab.parts.iter().map(|p| p.name.clone());
        let names: std::collections::BTreeSet<String> = names.collect();
        self.tab.expanded.retain(|n| names.contains(n));
        self.tab.listed.retain(|n, _| names.contains(n));
        // Что исчезло с диска, то и не выделено.
        let folders = tree_folders::visible(&self.tab.folders, &self.tab.parts, false);
        let loose = &self.tab.loose;
        self.tab.selection.retain(|n| match n {
            Node::Part(name) => names.contains(name),
            Node::Folder(path) => folders.contains(path),
            Node::File(path) => loose.contains(path),
        });
        let stale: Vec<String> = self
            .tab
            .expanded
            .iter()
            .filter(|n| Some(*n) != self.tab.part.as_ref())
            .filter(|n| {
                let entry = |list: &[PartEntry]| list.iter().find(|p| p.name == **n).cloned();
                entry(&old) != entry(&self.tab.parts)
            })
            .cloned()
            .collect();
        for name in stale {
            self.list_versions(ctx, &name);
        }
    }
}

/// Подходит ли деталь под поиск: без учёта регистра.
fn matches(name: &str, search: &str) -> bool {
    name.to_lowercase().contains(&search.trim().to_lowercase())
}

/// Строка детали: имя, последняя версия, сборка; несохранённое — рыжим.
fn part_row(ui: &mut egui::Ui, p: &PartEntry, selected: bool) -> egui::Response {
    let t = texts();
    let mut text = p.name.clone();
    if let Some(v) = &p.latest {
        text = format!("{text}   {v}");
    }
    if p.assembly {
        text = format!("{text}  ({})", t.assembly);
    }
    let mut rich = RichText::new(&text);
    if p.dirty {
        rich = rich.color(Color32::from_rgb(200, 120, 0));
    }
    let r = ui.selectable_label(selected, rich);
    selection::tell(&r, selected, &text);
    if p.dirty {
        r.on_hover_text(t.unsaved_changes)
    } else {
        r
    }
}

/// Деталь для меню правого щелчка: её модель или паспорт.
pub(super) fn target(p: &PartEntry) -> Target {
    Target {
        part: p.name.clone(),
        pick: Pick::Current,
        // Папку детали можно показать и без файла модели.
        path: p.model.clone().unwrap_or_else(|| META_FILE.to_string()),
        openable: p.model.is_some(),
        model_slot: true,
        removable: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_ignores_case_and_spaces() {
        assert!(matches("Кронштейн", " крон "));
        assert!(matches("кронштейн", ""));
        assert!(!matches("корпус", "крон"));
    }
}
