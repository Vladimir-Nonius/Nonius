//! Карточки «Структуры»: полоска вида, имя, версия или «заготовка»; обычные
//! файлы папки строками; подпись цветов. Карточка и заголовок папки — места,
//! куда бросают деталь из дерева или файл из Проводника.

use eframe::egui::{self, Color32, RichText};

use super::super::node::Node;
use super::super::shown::Shown;
use super::super::tree_folders::{self, Dragged};
use super::super::App;
use super::super::{open_menu, tree};
use super::{Acts, View};
use crate::source::PartEntry;
use crate::texts::texts;

/// Самая узкая карточка: уже — имя не помещается.
const CARD_MIN: f32 = 130.0;
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

/// Подпись цветов над структурой.
pub(super) fn legend(ui: &mut egui::Ui) {
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

/// Обычные файлы папки строками: двойной щелчок открывает.
pub(super) fn file_lines(app: &App, ui: &mut egui::Ui, files: &[&String], acts: &mut Acts) {
    for f in files {
        let text = RichText::new(format!("🗋 {}", tree_folders::last(f))).weak();
        let r = ui.selectable_label(false, text).on_hover_text(f.as_str());
        if r.double_clicked() {
            acts.open_file = Some(f.to_string());
        }
        let target = || tree_folders::loose_target(f);
        open_menu::attach(&r, &app.tab.programs, target, &mut acts.asked);
    }
}

/// Карточки рядами: рамки сами не переносятся, ряды считаем по ширине.
pub(super) fn cards(app: &mut App, ui: &mut egui::Ui, inside: &[&PartEntry], acts: &mut Acts) {
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
                let programs = &app.tab.programs;
                open_menu::attach(&name, programs, || tree::target(p), &mut acts.asked);
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
    acts.shown
        .push(Shown::new(rect, Node::Part(p.name.clone())));
    let stripe = egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height()));
    ui.painter().rect_filled(stripe, 0.0, kind.color(ui));
    // Брошена на карточку — в ту же папку, где лежит эта деталь.
    drop_here(ui, &inner.response, &p.folder, acts);
}

/// Сюда можно бросить деталь: в эту папку.
pub(super) fn drop_here(ui: &egui::Ui, r: &egui::Response, folder: &str, acts: &mut Acts) {
    if r.dnd_hover_payload::<Dragged>().is_some() {
        let stroke = ui.visuals().selection.stroke;
        ui.painter()
            .rect_stroke(r.rect, 3.0, stroke, egui::StrokeKind::Inside);
    }
    if let Some(d) = r.dnd_release_payload::<Dragged>() {
        acts.moved = Some((d.0.clone(), folder.to_string()));
    }
}
