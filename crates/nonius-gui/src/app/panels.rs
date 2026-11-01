//! Панели вокруг 3D-вида: «Что изменилось» справа, строка
//! состояния внизу и надписи поверх вида.

use eframe::egui::{self, Color32, RichText};

use super::legend::{legend, places};
use super::numbers::change_line;
use super::App;
use crate::texts::texts;

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

/// «Что изменилось» справа: легенда, места изменений, числа.
pub fn changes(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    // Без детали и без модели говорить не о чем: 3D занимает всё место.
    let model = app
        .parts
        .iter()
        .find(|p| Some(&p.name) == app.part.as_ref());
    if model.is_none_or(|p| p.model.is_none() && !p.assembly) {
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
