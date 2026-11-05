//! «Структура» в центре окна: проект по папкам. Каждая папка — блок в рамке
//! с заголовком и счётчиком, вложенная папка — внутри родителя. У детали
//! карточка с цветной полоской: серая — деталь, синяя — сборка, рыжая —
//! заготовка без модели (ей можно сразу добавить модель). Карточку
//! перетаскивают на заголовок другой папки так же, как строку в дереве.

use eframe::egui::{self, RichText};

use super::drop_zones::Zone;
use super::tree_folders;
use super::App;
use crate::source::PartEntry;
use crate::texts::texts;

/// Что показано в центре.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// Выбранная деталь в 3D.
    #[default]
    Part,
    /// Весь проект по папкам.
    Structure,
}

mod cards;

use cards::{cards, drop_here, file_lines, legend};

/// Сдвиг вложенной папки внутри родителя.
const NESTED: f32 = 16.0;

/// Переключатель над центром: «Деталь (3D)» или «Структура».
pub fn switch(app: &mut App, ui: &mut egui::Ui) {
    let t = texts();
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.tab.view, View::Part, t.view_part);
        ui.selectable_value(&mut app.tab.view, View::Structure, t.view_structure);
    });
}

/// Что сделали со структурой за кадр.
#[derive(Default)]
struct Acts {
    select: Option<String>,
    open: Option<String>,
    add_model: Option<String>,
    moved: Option<(String, String)>,
    zones: Vec<(egui::Rect, Zone)>,
    open_file: Option<String>,
}

/// Что рисовать: все папки и детали проекта.
struct Map {
    folders: Vec<String>,
    parts: Vec<PartEntry>,
    files: Vec<String>,
}

/// Нарисовать структуру проекта.
pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let mut acts = Acts::default();
    let parts = app.tab.parts.clone();
    let map = Map {
        folders: tree_folders::visible(&app.tab.folders, &parts, false),
        parts,
        files: app.tab.loose.clone(),
    };
    egui::ScrollArea::vertical()
        .id_salt("structure")
        .auto_shrink(false)
        .show(ui, |ui| {
            // Справа место под полосу прокрутки: иначе рамки уходят под неё.
            ui.set_max_width(ui.available_width() - 14.0);
            legend(ui);
            // Пустой корень не нужен: в нём просто ничего не лежит.
            let in_root = |f: &String| !f.contains('/');
            if map.parts.iter().any(|p| p.folder.is_empty()) || map.files.iter().any(in_root) {
                block(app, ui, &map, "", &mut acts);
            }
            for top in map.folders.iter().filter(|f| !f.contains('/')) {
                block(app, ui, &map, top, &mut acts);
            }
        });
    app.tab.zones.append(&mut acts.zones);
    if let Some(path) = acts.open_file {
        app.open_loose(ui.ctx(), path, false);
    }
    if let Some((part, folder)) = acts.moved {
        app.move_part(ui.ctx(), part, folder);
    }
    if let Some(name) = acts.add_model {
        let models = app.chooser.models();
        app.start_adding(models, Some(name));
    }
    if let Some(name) = acts.open.or(acts.select) {
        if app.tab.part.as_ref() != Some(&name) {
            app.select_part(ui.ctx(), &name);
        }
    }
}

/// Блок папки: заголовок со счётчиком, карточки, вложенные папки.
fn block(app: &mut App, ui: &mut egui::Ui, map: &Map, folder: &str, acts: &mut Acts) {
    let t = texts();
    let inside: Vec<&PartEntry> = map.parts.iter().filter(|p| p.folder == folder).collect();
    let files: Vec<&String> = map
        .files
        .iter()
        .filter(|f| tree_folders::parent(f) == folder)
        .collect();
    // У корня папки не вложены: они идут своими блоками ниже.
    let mut subs: Vec<&String> = map
        .folders
        .iter()
        .filter(|f| !folder.is_empty() && tree_folders::parent(f) == folder)
        .collect();
    subs.sort_by_key(|f| tree_folders::last(f).to_lowercase());
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    egui::Frame::NONE
        .stroke(stroke)
        .corner_radius(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let title = if folder.is_empty() {
                t.project_root.to_string()
            } else {
                format!("🗀 {}", tree_folders::last(folder))
            };
            let count = count(&inside, !subs.is_empty() || !files.is_empty());
            header(ui, &title, &count, folder, acts);
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    if inside.is_empty() && subs.is_empty() && files.is_empty() {
                        ui.label(RichText::new(t.empty_folder).weak());
                    }
                    cards(app, ui, &inside, acts);
                    file_lines(ui, &files, acts);
                    for sub in subs {
                        ui.horizontal_top(|ui| {
                            ui.add_space(NESTED);
                            ui.vertical(|ui| block(app, ui, map, sub, acts));
                        });
                    }
                });
        });
    ui.add_space(10.0);
}

/// Полоса заголовка: имя папки и счётчик; на неё бросают детали.
fn header(ui: &mut egui::Ui, title: &str, count: &str, folder: &str, acts: &mut Acts) {
    let fill = ui.visuals().faint_bg_color;
    let r = egui::Frame::NONE
        .fill(fill)
        .corner_radius(egui::CornerRadius {
            nw: 8,
            ne: 8,
            sw: 0,
            se: 0,
        })
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(count).weak());
                });
            });
        })
        .response;
    let line = ui.visuals().widgets.noninteractive.bg_stroke;
    ui.painter().hline(r.rect.x_range(), r.rect.bottom(), line);
    acts.zones.push((r.rect, Zone::Folder(folder.to_string())));
    drop_here(ui, &r, folder, acts);
}

/// «3 детали · 1 сборка» или «пусто».
fn count(inside: &[&PartEntry], has_subs: bool) -> String {
    let t = texts();
    let assemblies = inside.iter().filter(|p| p.assembly).count();
    let parts = inside.len() - assemblies;
    let mut out = Vec::new();
    if parts > 0 {
        out.push(format!("{parts} {}", plural(parts, t.parts_forms)));
    }
    if assemblies > 0 {
        out.push(format!(
            "{assemblies} {}",
            plural(assemblies, t.assemblies_forms)
        ));
    }
    if out.is_empty() && !has_subs {
        out.push(t.empty.to_string());
    }
    out.join(" · ")
}

/// Русское число: 1 деталь, 2 детали, 5 деталей.
fn plural(n: usize, forms: [&'static str; 3]) -> &'static str {
    match (n % 10, n % 100) {
        (1, x) if x != 11 => forms[0],
        (2..=4, x) if !(12..=14).contains(&x) => forms[1],
        _ => forms[2],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_plural() {
        let forms = ["деталь", "детали", "деталей"];
        let said: Vec<_> = [1, 2, 5, 11, 12, 21, 22, 25, 111]
            .iter()
            .map(|&n| plural(n, forms))
            .collect();
        assert_eq!(
            said,
            [
                "деталь",
                "детали",
                "деталей",
                "деталей",
                "деталей",
                "деталь",
                "детали",
                "деталей",
                "деталей"
            ]
        );
    }
}
