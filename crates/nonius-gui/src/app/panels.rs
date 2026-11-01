//! Панели вокруг 3D-вида: «Что изменилось» справа, строка
//! состояния внизу и надписи поверх вида.

use eframe::egui::{self, Color32, RichText};
use nonius_scene::Role;

use super::numbers::{change_line, volume_text};
use super::App;
use crate::texts::texts;
use crate::view::Model;

/// Полоса ошибки над окном: не заметить её нельзя, закрывается крестиком.
pub fn error_bar(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let Some(text) = app.message.clone() else {
        return;
    };
    let frame = egui::Frame::NONE
        .fill(Color32::from_rgb(251, 232, 232))
        .inner_margin(egui::Margin::symmetric(10, 6));
    egui::TopBottomPanel::top("error")
        .frame(frame)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("✕").on_hover_text(t.close).clicked() {
                    app.message = None;
                }
                let color = Color32::from_rgb(160, 30, 30);
                ui.add(egui::Label::new(RichText::new(text).color(color)).wrap());
            });
        });
}

/// Строка состояния: ход работы и «Отменить», что получилось или подсказка
/// по мыши.
pub fn status_bar(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let mut cancel = false;
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if app.jobs.busy() {
                ui.spinner();
                let p = app.jobs.progress();
                ui.label(if p.is_empty() {
                    t.comparing.to_string()
                } else {
                    p
                });
                if app.jobs.can_cancel() && ui.button(t.cancel_compare).clicked() {
                    cancel = true;
                }
            } else if let Some(n) = &app.notice {
                ui.label(n);
            } else if app.no_lfs {
                ui.label(RichText::new(t.no_lfs).weak());
            } else {
                ui.label(RichText::new(t.mouse_hint).weak());
            }
        });
    });
    if cancel {
        app.cancel_compare(ctx);
    }
}

/// «Что изменилось» справа: легенда, места изменений, числа. Пока деталь не
/// выбрана, панели нет: 3D занимает её место.
pub fn changes(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    if app.part.is_none() {
        return;
    }
    let mut zoom = None;
    egui::SidePanel::right("changes")
        .default_width(270.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(6.0);
                ui.heading(t.what_changed);
                let single = app.picks.0 == app.picks.1 && app.outside.is_none();
                if app.jobs.can_cancel() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t.calculating);
                    });
                } else if app.part.is_some() && single {
                    ui.label(RichText::new(t.nothing_to_compare).weak());
                } else if let (Some(model), Some(_)) = (app.viewport.model(), &app.comparison) {
                    legend(ui, model, &mut app.look);
                    if model.has_changes {
                        zoom = places(ui, model, &mut app.selected);
                    } else {
                        ui.add_space(4.0);
                        ui.label(t.no_shape_changes);
                    }
                }
                if let Some(c) = &app.comparison {
                    let warn = ui.visuals().warn_fg_color;
                    if c.step_behind {
                        ui.label(RichText::new(t.step_behind).color(warn));
                    }
                    if let Some(why) = c.scene_problem.as_ref().filter(|_| c.scene.is_some()) {
                        ui.label(RichText::new(format!("{}: {why}", t.no_colors)).color(warn));
                    }
                    let lines: Vec<_> = c.reports.iter().flat_map(|r| &r.changes).collect();
                    if !lines.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new(t.numbers).strong());
                        for ch in lines {
                            ui.label(change_line(ch));
                        }
                    }
                }
                if app.outside.is_none() {
                    super::material::show(app, ui);
                }
            });
        });
    if let Some(part) = zoom {
        app.viewport.zoom_to(ctx, part);
    }
}

fn swatch(ui: &mut egui::Ui, role: Role) {
    let [r, g, b] = role.color();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
}

/// Цвета видны всегда, даже когда такого изменения нет: иначе не понять, что
/// значит цвет. С объёмами; галочка прячет роль.
fn legend(ui: &mut egui::Ui, model: &Model, look: &mut crate::view::Look) {
    let t = texts();
    for (role, flag) in [
        (Role::Added, &mut look.added),
        (Role::Removed, &mut look.removed),
        (Role::Moved, &mut look.moved),
    ] {
        let volume: f64 = model
            .parts
            .iter()
            .filter(|p| p.role == role)
            .map(|p| p.volume_mm3)
            .sum();
        let present = model.parts.iter().any(|p| p.role == role);
        ui.horizontal(|ui| {
            ui.add_enabled(present, egui::Checkbox::without_text(flag));
            swatch(ui, role);
            if present {
                ui.label(format!("{}  {}", role_name(role), volume_text(volume)));
            } else {
                ui.label(RichText::new(role_name(role)).weak());
            }
        });
    }
    ui.horizontal(|ui| {
        ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
        swatch(ui, Role::After);
        ui.label(RichText::new(t.role_same).weak());
    });
}

/// Сколько мест показывать по одному. Дальше мелочь сводится строкой.
const PLACES_SHOWN: usize = 8;
/// Мелкое место: меньше этой доли от всего изменённого объёма.
const TINY_SHARE: f64 = 0.01;

/// Какие места сводятся строкой «Мелкие места»: только когда их много.
fn tiny_places(volumes: &[f64]) -> Vec<bool> {
    let total: f64 = volumes.iter().sum();
    let tiny: Vec<bool> = volumes.iter().map(|v| *v < total * TINY_SHARE).collect();
    let count = tiny.iter().filter(|t| **t).count();
    if volumes.len() > PLACES_SHOWN && count >= 2 {
        tiny
    } else {
        vec![false; volumes.len()]
    }
}

/// Список мест: ‹ › и щелчок — приблизить. Возвращает кусок для приближения.
fn places(ui: &mut egui::Ui, model: &Model, selected: &mut Option<usize>) -> Option<usize> {
    let t = texts();
    let list = model.places();
    if list.is_empty() {
        return None;
    }
    let mut zoom = None;
    let at = selected.and_then(|s| list.iter().position(|&i| i == s));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(t.places).strong());
        let n = list.len();
        if ui.small_button("‹").clicked() {
            zoom = Some(list[at.map_or(n - 1, |a| (a + n - 1) % n)]);
        }
        let shown = at.map_or("–".to_string(), |a| (a + 1).to_string());
        ui.label(format!("{shown} {} {n}", t.place_of));
        if ui.small_button("›").clicked() {
            zoom = Some(list[at.map_or(0, |a| (a + 1) % n)]);
        }
    });
    let volumes: Vec<f64> = list.iter().map(|&i| model.parts[i].volume_mm3).collect();
    let tiny = tiny_places(&volumes);
    let mut row = |ui: &mut egui::Ui, i: usize| {
        let p = &model.parts[i];
        ui.horizontal(|ui| {
            swatch(ui, p.role);
            let text = format!(
                "{} {}   {}",
                role_name(p.role),
                p.number,
                volume_text(p.volume_mm3)
            );
            if ui.selectable_label(*selected == Some(i), text).clicked() {
                zoom = Some(i);
            }
        });
    };
    for (k, &i) in list.iter().enumerate() {
        if !tiny[k] {
            row(ui, i);
        }
    }
    let small: Vec<usize> = (0..list.len()).filter(|&k| tiny[k]).collect();
    if !small.is_empty() {
        let volume: f64 = small.iter().map(|&k| volumes[k]).sum();
        let title = format!(
            "{}: {}, {}",
            t.tiny_places,
            small.len(),
            volume_text(volume)
        );
        egui::CollapsingHeader::new(title)
            .id_salt("tiny-places")
            .show(ui, |ui| {
                for &k in &small {
                    row(ui, list[k]);
                }
            });
    }
    if zoom.is_some() {
        *selected = zoom;
    }
    zoom
}

/// Пустой проект или деталь без модели: сказать и дать кнопку.
fn empty_state(app: &mut App, ui: &mut egui::Ui) -> bool {
    let t = texts();
    let waiting = app.jobs.busy() || app.add.is_some();
    let part = app
        .part
        .clone()
        .filter(|p| app.parts.iter().any(|e| e.name == *p && e.model.is_none()));
    let (text, label) = if app.parts.is_empty() {
        (t.empty_project, t.add_models)
    } else if part.is_some() {
        (t.empty_part, t.add_model)
    } else {
        return false;
    };
    if waiting {
        return true;
    }
    let rect = ui.max_rect();
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
        |ui| {
            ui.add_space(rect.height() * 0.4);
            ui.label(RichText::new(text).size(16.0).weak());
            ui.add_space(8.0);
            if ui.button(RichText::new(label).size(15.0)).clicked() {
                app.start_adding(app.chooser.models(), part);
            }
        },
    );
    true
}

/// Надписи поверх 3D: что выбрать, нечего показать, идёт расчёт.
pub fn overlay(app: &mut App, ui: &mut egui::Ui) {
    let t = texts();
    if empty_state(app, ui) {
        return;
    }
    let problem;
    let text = if !app.viewport.has_gpu() {
        Some(t.gpu_missing)
    } else if app.part.is_none() {
        Some(t.choose_part)
    } else if app.jobs.busy() && app.viewport.model().is_none() {
        Some(t.comparing_hint)
    } else if let Some(c) = app.comparison.as_ref().filter(|c| c.scene.is_none()) {
        let why = c.scene_problem.as_deref().unwrap_or(t.no_scene_why);
        problem = format!("{}\n\n{why}", t.no_scene);
        Some(problem.as_str())
    } else {
        None
    };
    if let Some(text) = text {
        let layout = egui::Layout::centered_and_justified(egui::Direction::TopDown);
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(ui.max_rect())
                .layout(layout),
            |ui| ui.add(egui::Label::new(RichText::new(text).size(16.0).weak()).selectable(false)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_places_fold_only_when_there_are_many() {
        assert_eq!(tiny_places(&[1000.0, 1.0, 1.0]), [false, false, false]);
        let mut many = vec![1000.0; 7];
        many.extend([1.0, 1.0, 1.0]);
        let folded = tiny_places(&many);
        assert_eq!(folded.iter().filter(|t| **t).count(), 3);
        assert!(!folded[0]);
    }
}

pub(super) fn role_name(role: Role) -> &'static str {
    let t = texts();
    match role {
        Role::After => t.role_after,
        Role::Before => t.role_before,
        Role::Added => t.role_added,
        Role::Removed => t.role_removed,
        Role::Moved => t.role_moved,
    }
}
