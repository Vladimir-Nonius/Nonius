//! «Структура» в центре окна: проект по папкам, у каждой детали карточка.
//! Деталь без модели — заготовка: ей можно сразу добавить модель. Карточку
//! перетаскивают на другую папку так же, как строку в дереве.

use eframe::egui::{self, RichText};

use super::tree_folders::{self, Dragged};
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

/// Ширина карточки детали.
const CARD: f32 = 170.0;

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
}

/// Нарисовать структуру проекта.
pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let t = texts();
    let mut acts = Acts::default();
    let parts = app.tab.parts.clone();
    let mut sections = vec![String::new()];
    sections.extend(tree_folders::visible(&app.tab.folders, &parts, false));
    egui::ScrollArea::vertical()
        .id_salt("structure")
        .auto_shrink(false)
        .show(ui, |ui| {
            for folder in &sections {
                let inside: Vec<&PartEntry> =
                    parts.iter().filter(|p| &p.folder == folder).collect();
                // Пустой корень не нужен: в нём просто ничего не лежит.
                if folder.is_empty() && inside.is_empty() {
                    continue;
                }
                ui.add_space(8.0);
                let title = if folder.is_empty() {
                    t.project_root.to_string()
                } else {
                    format!("🗀 {}", folder.replace('/', " / "))
                };
                let head = ui.add(
                    egui::Label::new(RichText::new(title).strong()).sense(egui::Sense::hover()),
                );
                drop_here(ui, &head, folder, &mut acts);
                if inside.is_empty() {
                    ui.label(RichText::new(t.empty_folder).weak());
                }
                // Рамки сами не переносятся на новую строку: ряды считаем по
                // ширине окна.
                let gap = ui.spacing().item_spacing.x;
                let per_row = ((ui.available_width() + gap) / (CARD + 16.0 + gap)).max(1.0);
                for row in inside.chunks(per_row as usize) {
                    ui.horizontal_top(|ui| {
                        for p in row {
                            card(app, ui, p, &mut acts);
                        }
                    });
                }
            }
        });
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

/// Сюда можно бросить деталь: в эту папку.
fn drop_here(ui: &egui::Ui, r: &egui::Response, folder: &str, acts: &mut Acts) {
    if r.dnd_hover_payload::<Dragged>().is_some() {
        let stroke = ui.visuals().selection.stroke;
        ui.painter()
            .rect_stroke(r.rect.expand(2.0), 3.0, stroke, egui::StrokeKind::Outside);
    }
    if let Some(d) = r.dnd_release_payload::<Dragged>() {
        acts.moved = Some((d.0.clone(), folder.to_string()));
    }
}

/// Карточка детали: имя, последняя версия или «заготовка».
fn card(app: &mut App, ui: &mut egui::Ui, p: &PartEntry, acts: &mut Acts) {
    let selected = app.tab.part.as_ref() == Some(&p.name);
    let frame = egui::Frame::group(ui.style());
    let inner = frame.show(ui, |ui| {
        ui.set_width(CARD);
        ui.vertical(|ui| card_lines(app, ui, p, selected, acts));
    });
    // Брошена на карточку — в ту же папку, где лежит эта деталь.
    drop_here(ui, &inner.response, &p.folder, acts);
}

/// Строки карточки: имя, версия или «заготовка», кнопка модели.
fn card_lines(app: &mut App, ui: &mut egui::Ui, p: &PartEntry, selected: bool, acts: &mut Acts) {
    let t = texts();
    let name = ui
        .selectable_label(selected, RichText::new(&p.name).strong())
        .on_hover_text(t.open_in_3d);
    let name = tree_folders::drag_part(name, p);
    if name.double_clicked() {
        acts.open = Some(p.name.clone());
        app.tab.view = View::Part;
    } else if name.clicked() {
        acts.select = Some(p.name.clone());
    }
    let mut line = match &p.latest {
        Some(v) => v.clone(),
        None if p.model.is_none() => t.blank_part.to_string(),
        None => t.not_saved.to_string(),
    };
    if p.assembly {
        line = format!("{line} · {}", t.assembly);
    }
    ui.label(RichText::new(line).weak());
    if p.model.is_none() && ui.small_button(t.add_model).clicked() {
        acts.add_model = Some(p.name.clone());
    }
}
