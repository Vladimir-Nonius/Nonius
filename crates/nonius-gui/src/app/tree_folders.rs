//! Папки в дереве, как в Obsidian: папка раскрывается стрелкой, деталь
//! перетаскивается мышью на папку, на другую деталь (в её папку) или на
//! пустое место внизу (в корень). Папка в дереве — папка на диске, её
//! обычные файлы (заметки, чертежи) — строки под деталями.

use std::collections::BTreeSet;

use eframe::egui::{self, RichText};

use super::drop_zones::Zone;
use super::new_folder::NewFolder;
use super::new_part::NewPart;
use super::open_menu::{self, Target};
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
        if ui
            .small_button(t.plus_part)
            .on_hover_text(t.new_part)
            .clicked()
        {
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
            acts.toggled = Some(FOLD_ALL.to_string());
        }
    });
}

/// Знак «свернуть всё» в `Acts::toggled`: у папки такого пути не бывает.
const FOLD_ALL: &str = "/";

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
        folder_row(app, ui, sub, open, acts);
        if open {
            ui.indent(("folder", sub), |ui| self::dir(app, ui, tree, sub, acts));
        }
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
        let chosen = app.tab.loose_chosen.as_deref() == Some(path);
        let text = RichText::new(format!("🗋 {}", last(path))).weak();
        let r = ui.selectable_label(chosen, text).on_hover_text(path);
        acts.zones
            .push((r.rect, Zone::Folder(parent(path).to_string())));
        if r.double_clicked() {
            acts.open_file = Some((path.to_string(), false));
        } else if r.clicked() || r.secondary_clicked() {
            acts.chosen_file = Some(path.to_string());
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

/// Строка папки: стрелка, имя; на неё бросают детали.
fn folder_row(app: &App, ui: &mut egui::Ui, path: &str, open: bool, acts: &mut Acts) {
    let t = texts();
    ui.horizontal(|ui| {
        // Стрелка — часть строки: щелчок по любому месту раскрывает папку.
        let arrow = if open { "▾" } else { "▸" };
        let chosen = app.tab.place == path;
        let r = ui.selectable_label(chosen, format!("{arrow} 🗀 {}", last(path)));
        acts.zones.push((r.rect, Zone::Folder(path.to_string())));
        drop_target(ui, &r, path, acts);
        if r.clicked() {
            acts.toggled = Some(path.to_string());
            acts.place = Some(path.to_string());
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
    });
}

/// Сюда можно бросить деталь: подсветить и запомнить, куда бросили.
fn drop_target(ui: &egui::Ui, r: &egui::Response, folder: &str, acts: &mut Acts) {
    if r.dnd_hover_payload::<Dragged>().is_some() {
        let stroke = ui.visuals().selection.stroke;
        ui.painter()
            .rect_stroke(r.rect, 3.0, stroke, egui::StrokeKind::Inside);
    }
    if let Some(d) = r.dnd_release_payload::<Dragged>() {
        acts.moved = Some((d.0.clone(), folder.to_string()));
    }
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

/// Пустое место под деревом: бросить сюда — в корень, щёлкнуть — новые
/// детали снова ложатся в корень.
pub(super) fn root_drop(ui: &mut egui::Ui, acts: &mut Acts) {
    let size = egui::vec2(ui.available_width(), ui.available_height().max(32.0));
    let r = ui.allocate_response(size, egui::Sense::click());
    if r.dnd_hover_payload::<Dragged>().is_some() {
        let stroke = ui.visuals().selection.stroke;
        ui.painter()
            .rect_stroke(r.rect, 3.0, stroke, egui::StrokeKind::Inside);
    }
    if let Some(d) = r.dnd_release_payload::<Dragged>() {
        acts.moved = Some((d.0.clone(), String::new()));
    }
    if r.clicked() {
        acts.place = Some(String::new());
    }
    acts.zones.push((r.rect, Zone::Folder(String::new())));
}

/// Исполнить то, что сделали с папками.
pub(super) fn apply(app: &mut App, ctx: &egui::Context, acts: &mut Acts) {
    app.tab.zones.append(&mut acts.zones);
    if let Some((path, folder)) = acts.open_file.take() {
        app.open_loose(ctx, path, folder);
    }
    if let Some(path) = acts.chosen_file.take() {
        app.tab.loose_chosen = Some(path);
    }
    // Delete по выделенному файлу — как в Проводнике, с вопросом.
    let delete = ctx.input(|i| i.key_pressed(egui::Key::Delete)) && !ctx.wants_keyboard_input();
    if let (true, Some(path)) = (delete, app.tab.loose_chosen.clone()) {
        if app.tab.loose.contains(&path) {
            app.ask_remove_file("", path);
        }
    }
    if let Some(path) = acts.toggled.take() {
        if path == FOLD_ALL {
            app.tab.folded = app.tab.folders.iter().cloned().collect();
            app.tab.expanded.clear();
        } else if !app.tab.folded.remove(&path) {
            app.tab.folded.insert(path);
        }
    }
    if let Some(place) = acts.place.take() {
        app.tab.place = place;
    }
    if let Some(folder) = acts.new_part.take() {
        app.tab.new_part = Some(NewPart::in_folder(&folder));
    }
    if let Some(folder) = acts.new_folder.take() {
        app.tab.new_folder = Some(NewFolder::in_folder(&folder));
    }
    if let Some((part, folder)) = acts.moved.take() {
        app.move_part(ctx, part, folder);
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
