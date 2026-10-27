//! Панели вокруг 3D-вида: детали слева, «Что изменилось» справа, строка
//! состояния внизу и надписи поверх вида.

use eframe::egui::{self, Color32, RichText};
use nonius_scene::Role;

use super::numbers::{change_line, volume_text};
use super::App;
use crate::texts::texts;
use crate::view::Model;

/// Строка состояния: ход работы, ошибка или подсказка по мыши.
pub fn status_bar(app: &mut App, ctx: &egui::Context) {
    let t = texts();
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
            } else if let Some(m) = &app.message {
                ui.label(RichText::new(m).color(ui.visuals().warn_fg_color));
            } else {
                ui.label(RichText::new(t.mouse_hint).weak());
            }
        });
    });
}

/// Список деталей слева.
pub fn parts(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let mut clicked = None;
    egui::SidePanel::left("parts")
        .default_width(220.0)
        .show(ctx, |ui| {
            if let Some(p) = &app.project {
                ui.add_space(6.0);
                ui.heading(p.title());
            }
            ui.label(RichText::new(t.parts).strong());
            ui.separator();
            if app.parts.is_empty() && !app.jobs.busy() {
                ui.label(RichText::new(t.no_parts).weak());
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for p in &app.parts {
                    let mut text = p.name.clone();
                    if let Some(v) = &p.latest {
                        text = format!("{text}   {v}");
                    }
                    if p.assembly {
                        text = format!("{text}  ({})", t.assembly);
                    }
                    let mut rich = RichText::new(text);
                    if p.dirty {
                        rich = rich.color(Color32::from_rgb(200, 120, 0));
                    }
                    let selected = app.part.as_ref() == Some(&p.name);
                    let mut r = ui.selectable_label(selected, rich);
                    if p.dirty {
                        r = r.on_hover_text(t.unsaved_changes);
                    }
                    if r.clicked() && !selected {
                        clicked = Some(p.name.clone());
                    }
                }
            });
        });
    if let Some(name) = clicked {
        app.select_part(ctx, &name);
    }
}

/// «Что изменилось» справа: легенда, места изменений, числа, настройки вида.
pub fn changes(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let mut zoom = None;
    let mut save = false;
    egui::SidePanel::right("changes")
        .default_width(270.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(6.0);
                ui.heading(t.what_changed);
                if let Some(model) = app.viewport.model() {
                    if model.has_changes {
                        legend(ui, model, &mut app.look);
                        zoom = places(ui, model, &mut app.selected);
                    } else {
                        ui.label(t.no_shape_changes);
                    }
                }
                if let Some(c) = &app.comparison {
                    let lines: Vec<_> = c.reports.iter().flat_map(|r| &r.changes).collect();
                    if !lines.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new(t.numbers).strong());
                        for ch in lines {
                            ui.label(change_line(ch));
                        }
                    }
                }
                ui.add_space(8.0);
                ui.separator();
                ui.checkbox(&mut app.look.xray, t.xray);
                ui.checkbox(&mut app.look.edges, t.edges);
                if ui
                    .checkbox(&mut app.settings.wheel_reverse, t.wheel_reverse)
                    .changed()
                {
                    app.settings.save();
                }
                ui.add_space(8.0);
                if ui.button(t.save_picture).clicked() {
                    save = true;
                }
            });
        });
    if let Some(part) = zoom {
        app.viewport.zoom_to(ctx, part);
    }
    if save {
        super::toolbar::save_picture(app);
    }
}

fn swatch(ui: &mut egui::Ui, role: Role) {
    let [r, g, b] = role.color();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
}

/// Цвета с объёмами; галочка прячет роль.
fn legend(ui: &mut egui::Ui, model: &Model, look: &mut crate::view::Look) {
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
        if !model.parts.iter().any(|p| p.role == role) {
            continue;
        }
        ui.horizontal(|ui| {
            ui.checkbox(flag, "");
            swatch(ui, role);
            ui.label(format!("{}  {}", role_name(role), volume_text(volume)));
        });
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
    for &i in &list {
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
    }
    if zoom.is_some() {
        *selected = zoom;
    }
    zoom
}

/// Надписи поверх 3D: что выбрать, нечего показать, идёт сравнение.
pub fn overlay(app: &App, ui: &mut egui::Ui) {
    let t = texts();
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

fn role_name(role: Role) -> &'static str {
    let t = texts();
    match role {
        Role::After => t.role_after,
        Role::Before => t.role_before,
        Role::Added => t.role_added,
        Role::Removed => t.role_removed,
        Role::Moved => t.role_moved,
    }
}
