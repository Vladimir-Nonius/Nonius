//! Карточки «Структуры»: полоска вида, имя, версия или «заготовка»; обычные
//! файлы папки строками; подпись цветов. Карточка отмечает своё место:
//! на неё бросают файл из Проводника, а деталь — в её папку.

use eframe::egui::{self, Color32, RichText};

use super::super::node::Node;
use super::super::selection::{self, CORNER};
use super::super::shown::Shown;
use super::super::tree_folders;
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

/// Обычные файлы папки строками: щелчок выделяет, двойной открывает.
pub(super) fn file_lines(app: &App, ui: &mut egui::Ui, files: &[&String], acts: &mut Acts) {
    for f in files {
        let node = Node::File(f.to_string());
        let chosen = app.tab.selection.contains(&node);
        let text = RichText::new(format!("🗋 {}", tree_folders::last(f))).weak();
        let r = ui.selectable_label(chosen, text).on_hover_text(f.as_str());
        acts.shown.push(Shown::on(ui, r.rect, node.clone()));
        if r.double_clicked() {
            acts.open_file = Some(f.to_string());
        } else if r.clicked() || r.secondary_clicked() {
            acts.picked = Some((node.clone(), r.secondary_clicked()));
        }
        let many = app.tab.selection.many_with(&node);
        let busy = app.tab.batch.is_some();
        let target = || tree_folders::loose_target(f);
        open_menu::attach_node(&r, &app.tab.programs, target, many, busy, &mut acts.asked);
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

/// Карточка детали: полоска вида, имя, версия или «заготовка». Вся
/// карточка — одно целое: щелчок, двойной щелчок, правая кнопка и
/// перетаскивание за любое её место, а не только за имя.
fn card(app: &mut App, ui: &mut egui::Ui, p: &PartEntry, width: f32, acts: &mut Acts) {
    let t = texts();
    let kind = Kind::of(p);
    let node = Node::Part(p.name.clone());
    let chosen = app.tab.selection.contains(&node);
    let opened = app.tab.part.as_ref() == Some(&p.name);
    let visuals = ui.visuals().clone();
    let fill = if chosen {
        visuals.selection.bg_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    let mut slot = None;
    let inner = egui::Frame::NONE
        .fill(fill)
        .corner_radius(CORNER)
        .stroke(visuals.widgets.noninteractive.bg_stroke)
        .inner_margin(egui::Margin {
            left: 10,
            right: 6,
            top: 4,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.set_width(width - 16.0);
            ui.vertical(|ui| {
                // Надписи не ловят мышь: её ловит карточка целиком.
                let name = RichText::new(&p.name).strong();
                ui.add(egui::Label::new(name).selectable(false));
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
                let line = RichText::new(line).small().color(color);
                ui.add(egui::Label::new(line).selectable(false));
                if kind == Kind::Blank {
                    // Место под кнопку: сама кнопка ляжет поверх карточки.
                    slot = Some(ui.allocate_space(add_model_size(ui)).1);
                }
            });
        });
    let r = inner
        .response
        .interact(egui::Sense::click_and_drag())
        .on_hover_text(t.open_in_3d);
    let r = tree_folders::drag_part(r, p);
    let many = app.tab.selection.many_with(&node);
    let busy = app.tab.batch.is_some();
    let target = || tree::target(p);
    open_menu::attach_node(&r, &app.tab.programs, target, many, busy, &mut acts.asked);
    if r.double_clicked() {
        acts.open = Some(p.name.clone());
        app.tab.view = View::Part;
    } else if r.clicked() || r.secondary_clicked() {
        acts.picked = Some((node.clone(), r.secondary_clicked()));
    }
    if let Some(slot) = slot {
        let button = egui::Button::new(RichText::new(t.add_model).small());
        if ui.put(slot, button).clicked() {
            acts.add_model = Some(p.name.clone());
        }
    }
    let rect = r.rect;
    // Открыта, но выделено другое — тонкая рамка; под мышью — рамка ярче.
    if opened && !chosen {
        selection::opened_frame(ui, rect);
    } else if r.hovered() && !chosen {
        let stroke = visuals.widgets.hovered.bg_stroke;
        ui.painter()
            .rect_stroke(rect, CORNER, stroke, egui::StrokeKind::Inside);
    }
    acts.shown.push(Shown::on(ui, rect, node));
    let stripe = egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height()));
    // Полоска слева повторяет скругление карточки.
    let round = egui::CornerRadius {
        nw: CORNER as u8,
        sw: CORNER as u8,
        ne: 0,
        se: 0,
    };
    ui.painter().rect_filled(stripe, round, kind.color(ui));
}

/// Размер кнопки «Добавить модель…» мелким шрифтом.
fn add_model_size(ui: &egui::Ui) -> egui::Vec2 {
    let text = egui::WidgetText::from(RichText::new(texts().add_model).small());
    let galley = text.into_galley(ui, None, f32::INFINITY, egui::TextStyle::Small);
    galley.size() + 2.0 * ui.spacing().button_padding
}
