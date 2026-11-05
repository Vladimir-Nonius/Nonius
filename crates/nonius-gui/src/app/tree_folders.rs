//! Папки в дереве, как в Obsidian: папка раскрывается стрелкой, деталь
//! перетаскивается мышью на папку, на другую деталь (в её папку) или на
//! пустое место внизу (в корень); куда она ляжет, решает `drag_hint.rs` по
//! отмеченным местам. Папка в дереве — папка на диске, её обычные файлы
//! (заметки, чертежи) — строки под деталями.

use std::collections::BTreeSet;

use eframe::egui::{self, RichText};

use super::model_names::where_words;
use super::new_folder::NewFolder;
use super::new_part::NewPart;
use super::node::Node;
use super::open_menu::{self, Target};
use super::selection;
use super::shown::{self, Shown};
use super::tree::{part_line, Acts};
use super::App;
use crate::source::{PartEntry, Pick};
use crate::texts::texts;

/// Что тащат мышью: имя детали.
pub(super) struct Dragged(pub String);

/// Что рисовать: папки и детали после поиска.
pub(super) struct Tree<'a> {
    pub folders: &'a [String],
    pub parts: &'a [PartEntry],
    /// Обычные файлы папок, путь от корня через `/`.
    pub files: &'a [String],
    /// Идёт поиск: видны только папки с найденным, все раскрыты.
    pub searching: bool,
}

/// Кнопки над деревом: новая деталь, новая папка, свернуть всё.
pub(super) fn buttons(app: &App, ui: &mut egui::Ui, acts: &mut Acts) {
    let t = texts();
    ui.horizontal(|ui| {
        let place = &app.tab.place;
        // Короткие надписи: панель узкая, полное название — в подсказке.
        let tip = format!("{} {}", t.new_part_tip, where_words(place));
        if ui.small_button(t.plus_part).on_hover_text(tip).clicked() {
            acts.new_part = Some(place.clone());
        }
        if ui
            .small_button(t.plus_folder)
            .on_hover_text(t.new_folder)
            .clicked()
        {
            acts.new_folder = Some(place.clone());
        }
        if ui.small_button("⏶").on_hover_text(t.fold_all).clicked() {
            acts.fold_all = true;
        }
    });
}

/// Папки, которые видны: все, а при поиске — только те, где что-то нашлось.
/// Папки деталей добавляются, даже если их нет в списке (старый проект).
pub(super) fn visible(folders: &[String], parts: &[PartEntry], searching: bool) -> Vec<String> {
    let mut out: BTreeSet<String> = if searching {
        BTreeSet::new()
    } else {
        folders.iter().cloned().collect()
    };
    for p in parts {
        let mut path = String::new();
        for name in p.folder.split('/').filter(|s| !s.is_empty()) {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(name);
            out.insert(path.clone());
        }
    }
    out.into_iter().collect()
}

/// Папка над `path`: пустая строка — корень.
pub(super) fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(p, _)| p)
}

/// Последнее имя в пути.
pub(super) fn last(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Содержимое папки `dir`: сначала папки, потом детали.
pub(super) fn dir(app: &mut App, ui: &mut egui::Ui, tree: &Tree, dir: &str, acts: &mut Acts) {
    let mut subs: Vec<&String> = tree.folders.iter().filter(|f| parent(f) == dir).collect();
    subs.sort_by_key(|f| last(f).to_lowercase());
    for sub in subs {
        let open = tree.searching || !app.tab.folded.contains(sub);
        // Папка в списке раньше содержимого: Shift считает по этому
        // порядку. Место узнаем, когда содержимое нарисуется.
        let at = acts.shown.len();
        let node = Node::Folder(sub.clone());
        acts.shown
            .push(Shown::new(egui::Rect::NOTHING, node.clone()));
        let row = folder_row(app, ui, sub, open, acts);
        // Сама строка — для рамки мышью: задета строка, а не вся папка.
        acts.shown.push(Shown::on(ui, row, node.clone()));
        let mut bottom = row.bottom();
        if open {
            let inside = ui.indent(("folder", sub), |ui| self::dir(app, ui, tree, sub, acts));
            bottom = inside.response.rect.bottom();
        }
        // Папка целиком — строка и всё, что под ней, во всю ширину дерева:
        // её подсвечивают, когда бросают на файл или деталь внутри.
        let x = ui.max_rect().x_range();
        let whole = egui::Rect::from_x_y_ranges(x, row.top()..=bottom);
        acts.shown[at] = Shown::on(ui, whole, node);
    }
    for p in tree.parts.iter().filter(|p| p.folder == dir) {
        part_line(app, ui, p, acts);
    }
    for f in tree.files.iter().filter(|f| parent(f) == dir) {
        file_row(app, ui, f, acts);
    }
}

/// Строка обычного файла: щелчок выделяет (Delete — удалить), двойной
/// открывает, правый — то же меню, что у файлов детали.
fn file_row(app: &App, ui: &mut egui::Ui, path: &str, acts: &mut Acts) {
    ui.horizontal(|ui| {
        // Под стрелкой папки или детали: файл раскрывать некуда.
        ui.add_space(14.0);
        let node = Node::File(path.to_string());
        let chosen = app.tab.selection.contains(&node);
        let text = format!("🗋 {}", last(path));
        let r = ui.selectable_label(chosen, RichText::new(&text).weak());
        selection::tell(&r, chosen, &text);
        let r = r.on_hover_text(path);
        acts.shown.push(Shown::on(ui, r.rect, node.clone()));
        if r.double_clicked() {
            acts.open_file = Some((path.to_string(), false));
        } else if r.clicked() || r.secondary_clicked() {
            acts.picked = Some((node, r.secondary_clicked()));
        }
        open_menu::attach(
            &r,
            &app.tab.programs,
            || loose_target(path),
            &mut acts.asked,
        );
    });
}

/// Обычный файл папки для меню правого щелчка.
pub(super) fn loose_target(path: &str) -> Target {
    Target {
        part: String::new(),
        pick: Pick::Current,
        path: path.to_string(),
        openable: true,
        model_slot: false,
        removable: true,
    }
}

/// Строка папки: стрелка, имя; на неё бросают детали. Отдаёт место строки.
fn folder_row(app: &App, ui: &mut egui::Ui, path: &str, open: bool, acts: &mut Acts) -> egui::Rect {
    let t = texts();
    ui.horizontal(|ui| {
        // Стрелка — часть строки: щелчок по любому месту раскрывает папку.
        let arrow = if open { "▾" } else { "▸" };
        let node = Node::Folder(path.to_string());
        let chosen = app.tab.selection.contains(&node);
        let text = format!("{arrow} 🗀 {}", last(path));
        let r = ui.selectable_label(chosen, &text);
        selection::tell(&r, chosen, &text);
        if r.clicked() || r.secondary_clicked() {
            acts.picked = Some((node, r.secondary_clicked()));
        }
        r.context_menu(|ui| {
            if ui.button(t.new_part_here).clicked() {
                ui.close();
                acts.new_part = Some(path.to_string());
            }
            if ui.button(t.new_folder).clicked() {
                ui.close();
                acts.new_folder = Some(path.to_string());
            }
        });
    })
    .response
    .rect
}

/// Строку детали можно тащить мышью. Щелчок остаётся щелчком: тащить
/// начинают, только сдвинув мышь. Та же строка слушает и щелчок, и
/// перетаскивание: второй виджет поверх неё забрал бы щелчки.
pub(super) fn drag_part(row: egui::Response, p: &PartEntry) -> egui::Response {
    let row = row.interact(egui::Sense::drag());
    row.dnd_set_drag_payload(Dragged(p.name.clone()));
    if row.dragged() {
        // Строка на старом месте бледнеет: видно, что её несут.
        let fill = row.ctx.style().visuals.panel_fill.gamma_multiply(0.7);
        let painter = row.ctx.layer_painter(row.layer_id);
        painter.rect_filled(row.rect, 3.0, fill);
    }
    row
}

/// Пустое место под деревом: бросить сюда — в корень, щёлкнуть — снять
/// выделение, и новые детали снова ложатся в корень.
pub(super) fn root_drop(ui: &mut egui::Ui, acts: &mut Acts) {
    let size = egui::vec2(ui.available_width(), ui.available_height().max(32.0));
    let r = ui.allocate_response(size, egui::Sense::click());
    if r.clicked() {
        acts.picked = Some((Node::Folder(String::new()), false));
    }
    acts.shown
        .push(Shown::on(ui, r.rect, Node::Folder(String::new())));
}

/// Исполнить то, что сделали с папками.
pub(super) fn apply(app: &mut App, ctx: &egui::Context, acts: &mut Acts) {
    let order = shown::order(&acts.shown);
    app.sweep(ctx, &acts.sweep, &acts.shown, true);
    if acts.sweep.clicked {
        // Щелчок справа от строк — как по пустому месту внизу.
        acts.picked = Some((Node::Folder(String::new()), false));
    }
    app.tab.on_screen.append(&mut acts.shown);
    if let Some((path, folder)) = acts.open_file.take() {
        app.open_loose(ctx, path, folder);
    }
    if let Some((node, secondary)) = acts.picked.take() {
        app.pick(ctx, node, secondary, &order, true);
    }
    // Delete по выделенному — как в Проводнике, с вопросом. Последним
    // выбрали файл на вкладке «Файлы» — Delete про него, это решает она.
    let delete = ctx.input(|i| i.key_pressed(egui::Key::Delete))
        && !ctx.wants_keyboard_input()
        && app.tab.dialog.is_none()
        && !app.tab.keys_to_files;
    if delete {
        app.delete_selected();
    }
    if std::mem::take(&mut acts.fold_all) {
        app.tab.folded = app.tab.folders.iter().cloned().collect();
        app.tab.expanded.clear();
    }
    if let Some(folder) = acts.new_part.take() {
        app.tab.new_part = Some(NewPart::in_folder(&folder));
    }
    if let Some(folder) = acts.new_folder.take() {
        app.tab.new_folder = Some(NewFolder::in_folder(&folder));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str, folder: &str) -> PartEntry {
        PartEntry {
            name: name.into(),
            assembly: false,
            latest: None,
            dirty: false,
            model: None,
            stamp: String::new(),
            folder: folder.into(),
        }
    }

    #[test]
    fn folders_with_found_parts_only_while_searching() {
        let folders = vec!["Стена".to_string(), "Стол".to_string()];
        let parts = vec![part("RAIL-01", "Стена/Рейка")];
        assert_eq!(
            visible(&folders, &parts, false),
            ["Стена", "Стена/Рейка", "Стол"]
        );
        assert_eq!(visible(&folders, &parts, true), ["Стена", "Стена/Рейка"]);
    }

    #[test]
    fn paths() {
        assert_eq!(parent("Стена/Рейка"), "Стена");
        assert_eq!(parent("Стена"), "");
        assert_eq!(last("Стена/Рейка"), "Рейка");
    }
}
