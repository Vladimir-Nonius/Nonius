//! «Структура» в центре окна: проект по папкам. Каждая папка — блок в рамке
//! с заголовком и счётчиком, вложенная папка — внутри родителя. У детали
//! карточка с цветной полоской: серая — деталь, синяя — сборка, рыжая —
//! заготовка без модели (ей можно сразу добавить модель). Карточку
//! перетаскивают на заголовок другой папки так же, как строку в дереве.

use eframe::egui::{self, Color32, RichText};

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

/// Самая узкая карточка: уже — имя не помещается.
const CARD_MIN: f32 = 130.0;
/// Сдвиг вложенной папки внутри родителя.
const NESTED: f32 = 16.0;
/// Заготовка — тем же рыжим, что несохранённое в дереве.
const BLANK: Color32 = Color32::from_rgb(200, 120, 0);

/// Вид карточки: от него цвет полоски.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Part,
    Assembly,
    Blank,
}

impl Kind {
    fn of(p: &PartEntry) -> Self {
        match (p.model.is_none() && p.latest.is_none(), p.assembly) {
            (true, _) => Self::Blank,
            (false, true) => Self::Assembly,
            (false, false) => Self::Part,
        }
    }

    fn color(self, ui: &egui::Ui) -> Color32 {
        match self {
            Self::Part => ui.visuals().weak_text_color(),
            Self::Assembly => ui.visuals().hyperlink_color,
            Self::Blank => BLANK,
        }
    }
}

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

/// Что рисовать: все папки и детали проекта.
struct Map {
    folders: Vec<String>,
    parts: Vec<PartEntry>,
}

/// Нарисовать структуру проекта.
pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let mut acts = Acts::default();
    let parts = app.tab.parts.clone();
    let map = Map {
        folders: tree_folders::visible(&app.tab.folders, &parts, false),
        parts,
    };
    egui::ScrollArea::vertical()
        .id_salt("structure")
        .auto_shrink(false)
        .show(ui, |ui| {
            // Справа место под полосу прокрутки: иначе рамки уходят под неё.
            ui.set_max_width(ui.available_width() - 14.0);
            legend(ui);
            // Пустой корень не нужен: в нём просто ничего не лежит.
            if map.parts.iter().any(|p| p.folder.is_empty()) {
                block(app, ui, &map, "", &mut acts);
            }
            for top in map.folders.iter().filter(|f| !f.contains('/')) {
                block(app, ui, &map, top, &mut acts);
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

/// Подпись цветов над структурой.
fn legend(ui: &mut egui::Ui) {
    let t = texts();
    ui.horizontal(|ui| {
        for (kind, text) in [
            (Kind::Part, t.legend_part),
            (Kind::Assembly, t.assembly),
            (Kind::Blank, t.legend_blank),
        ] {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, kind.color(ui));
            ui.label(RichText::new(text).weak());
            ui.add_space(8.0);
        }
    });
    ui.add_space(4.0);
}

/// Блок папки: заголовок со счётчиком, карточки, вложенные папки.
fn block(app: &mut App, ui: &mut egui::Ui, map: &Map, folder: &str, acts: &mut Acts) {
    let t = texts();
    let inside: Vec<&PartEntry> = map.parts.iter().filter(|p| p.folder == folder).collect();
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
            header(ui, &title, &count(&inside, !subs.is_empty()), folder, acts);
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    if inside.is_empty() && subs.is_empty() {
                        ui.label(RichText::new(t.empty_folder).weak());
                    }
                    cards(app, ui, &inside, acts);
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

/// Карточки рядами: рамки сами не переносятся, ряды считаем по ширине.
fn cards(app: &mut App, ui: &mut egui::Ui, inside: &[&PartEntry], acts: &mut Acts) {
    if inside.is_empty() {
        return;
    }
    let gap = ui.spacing().item_spacing.x;
    let width = ui.available_width();
    let per_row = ((width + gap) / (CARD_MIN + gap)).floor().max(1.0);
    let card = (width + gap) / per_row - gap;
    for row in inside.chunks(per_row as usize) {
        ui.horizontal_top(|ui| {
            for p in row {
                self::card(app, ui, p, card, acts);
            }
        });
    }
    ui.add_space(4.0);
}

/// Карточка детали: полоска вида, имя, версия или «заготовка».
fn card(app: &mut App, ui: &mut egui::Ui, p: &PartEntry, width: f32, acts: &mut Acts) {
    let t = texts();
    let kind = Kind::of(p);
    let selected = app.tab.part.as_ref() == Some(&p.name);
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    let inner = egui::Frame::NONE
        .stroke(stroke)
        .inner_margin(egui::Margin {
            left: 10,
            right: 6,
            top: 4,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.set_width(width - 16.0);
            ui.vertical(|ui| {
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
                let version = match (&p.latest, kind) {
                    (Some(v), _) => v.clone(),
                    (None, Kind::Blank) => t.blank_part.to_string(),
                    (None, _) => t.not_saved.to_string(),
                };
                let line = if p.assembly {
                    format!("{} · {version}", t.assembly)
                } else {
                    version
                };
                let color = match kind {
                    Kind::Part => ui.visuals().weak_text_color(),
                    other => other.color(ui),
                };
                ui.label(RichText::new(line).small().color(color));
                if kind == Kind::Blank && ui.small_button(t.add_model).clicked() {
                    acts.add_model = Some(p.name.clone());
                }
            });
        });
    let rect = inner.response.rect;
    let stripe = egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height()));
    ui.painter().rect_filled(stripe, 0.0, kind.color(ui));
    // Брошена на карточку — в ту же папку, где лежит эта деталь.
    drop_here(ui, &inner.response, &p.folder, acts);
}

/// Сюда можно бросить деталь: в эту папку.
fn drop_here(ui: &egui::Ui, r: &egui::Response, folder: &str, acts: &mut Acts) {
    if r.dnd_hover_payload::<Dragged>().is_some() {
        let stroke = ui.visuals().selection.stroke;
        ui.painter()
            .rect_stroke(r.rect, 3.0, stroke, egui::StrokeKind::Inside);
    }
    if let Some(d) = r.dnd_release_payload::<Dragged>() {
        acts.moved = Some((d.0.clone(), folder.to_string()));
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
