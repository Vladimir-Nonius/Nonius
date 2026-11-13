//! «Показать в 3D» на экране разбора: две модели из папки — тем же
//! 3D-видом, которым окно сравнивает версии детали (общее серым, убранное
//! красным, добавленное зелёным), а под картинкой — что изменилось словами.
//!
//! Сравнение идёт в фоне со своей строкой хода и «Остановить»: для файла
//! SolidWorks STEP может делать сам SolidWorks. У экрана свой 3D-вид
//! (`App::intake_view`): камера и модель детали в главном виде не трогаются.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText};

use super::{Stage, Steps};
use crate::app::jobs::Done;
use crate::app::App;
use crate::source::{Comparison, IntakeSide};
use crate::texts::texts;
use crate::view::{Mode, Model};

/// Открытое сравнение двух моделей.
pub(in crate::app) struct Sight {
    /// Номер показа: результат прошлого, закрытого, сюда не попадёт.
    id: u64,
    /// Подписи «было» и «стало».
    names: (String, String),
    /// Что изменилось словами — то, что посчитал разбор.
    words: Vec<String>,
    /// `None` — ещё считается; ошибка — словами.
    result: Option<Result<Comparison, String>>,
    mode: Mode,
    stop: Arc<AtomicBool>,
    line: Arc<Mutex<String>>,
}

impl Sight {
    /// Ждёт ли результата.
    pub fn waiting(&self) -> bool {
        self.result.is_none()
    }

    /// Остановить расчёт: результат уже не нужен.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Строки «что изменилось»: слова разбора, а нет их — отчёты сравнения.
    fn lines(&self) -> Vec<String> {
        if !self.words.is_empty() {
            return self.words.clone();
        }
        let Some(Ok(c)) = &self.result else {
            return Vec::new();
        };
        let changes = c.reports.iter().flat_map(|r| r.changes.iter());
        changes.map(ToString::to_string).collect()
    }
}

impl App {
    /// «Показать в 3D»: сравнить две модели в фоне.
    pub(super) fn intake_look(
        &mut self,
        ctx: &egui::Context,
        before: IntakeSide,
        after: IntakeSide,
        words: Vec<String>,
    ) {
        let (Some(project), Some(intake)) = (self.tab.project.clone(), self.tab.intake.as_mut())
        else {
            return;
        };
        if let Some(old) = intake.sight.take() {
            old.stop();
        }
        intake.sights += 1;
        let (run, id) = (intake.run, intake.sights);
        let stop = Arc::new(AtomicBool::new(false));
        let line = Arc::new(Mutex::new(texts().intake_3d_working.to_string()));
        intake.sight = Some(Sight {
            id,
            names: (before.label, after.label),
            words,
            result: None,
            mode: Mode::Changes,
            stop: stop.clone(),
            line: line.clone(),
        });
        let steps = Steps {
            ctx: ctx.clone(),
            line,
            stop,
        };
        let (was, now) = (before.models, after.models);
        self.tab.jobs.spawn(ctx, move |_| {
            Done::IntakeLooked(run, id, project.intake_compare(&was, &now, &steps))
        });
    }

    /// Сравнение готово: модель — в свой 3D-вид экрана.
    pub(in crate::app) fn intake_looked(
        &mut self,
        run: u64,
        id: u64,
        result: anyhow::Result<Comparison>,
    ) {
        let Some(sight) = self
            .intake_of(run)
            .and_then(|i| i.sight.as_mut())
            .filter(|s| s.id == id)
        else {
            return;
        };
        let model = result
            .as_ref()
            .ok()
            .and_then(|c| c.scene.as_ref())
            .and_then(Model::new);
        sight.result = Some(result.map_err(|e| format!("{e:#}")));
        let view = self.intake_view.get_or_insert_with(|| self.viewport.twin());
        view.set_model(model, false);
    }
}

/// Кнопки «Показать в 3D»: на каждую пару соседних версий своя. Нажатая —
/// «было», «стало» и слова разбора между ними.
pub(super) fn buttons(
    ui: &mut egui::Ui,
    sides: &[IntakeSide],
    changed: &[Vec<String>],
) -> Option<(IntakeSide, IntakeSide, Vec<String>)> {
    let t = texts();
    let mut pressed = None;
    ui.horizontal_wrapped(|ui| {
        for (n, pair) in sides.windows(2).enumerate() {
            let label = match sides.len() {
                2 => t.intake_show_3d.to_string(),
                _ => format!(
                    "{}: «{}» → «{}»",
                    t.intake_show_3d, pair[0].label, pair[1].label
                ),
            };
            if ui.button(label).clicked() {
                let words = changed.get(n).cloned().unwrap_or_default();
                pressed = Some((pair[0].clone(), pair[1].clone(), words));
            }
        }
    });
    pressed
}

/// Что нажали на экране сравнения.
enum Act {
    Back,
    Stop,
    Fit,
}

/// Нарисовать экран сравнения; `false` — его нет.
pub fn show(app: &mut App, ui: &mut egui::Ui) -> bool {
    let t = texts();
    let open = app
        .tab
        .intake
        .as_ref()
        .is_some_and(|i| matches!(i.stage, Stage::Plan) && i.sight.is_some());
    if !open {
        return false;
    }
    let mut act = None;
    egui::Frame::NONE.inner_margin(12).show(ui, |ui| {
        let Some(sight) = app.tab.intake.as_mut().and_then(|i| i.sight.as_mut()) else {
            return;
        };
        ui.horizontal(|ui| {
            ui.heading(t.intake_3d_title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t.intake_back).clicked() {
                    act = Some(Act::Back);
                }
            });
        });
        let (a, b) = &sight.names;
        ui.label(RichText::new(format!("«{a}» → «{b}»")).strong());
        ui.separator();
        match &sight.result {
            None => {
                working(ui, sight, &mut act);
                return;
            }
            Some(Err(e)) => {
                ui.add_space(8.0);
                let red = ui.visuals().error_fg_color;
                ui.add(egui::Label::new(RichText::new(e).color(red)).wrap());
                return;
            }
            Some(Ok(_)) => {}
        }
        let view = app.intake_view.get_or_insert_with(|| app.viewport.twin());
        let changes = view.model().is_some_and(|m| m.has_changes);
        let has_model = view.model().is_some();
        let Some(Ok(c)) = &sight.result else {
            return;
        };
        let (problem, behind) = (c.scene_problem.clone(), c.step_behind);
        ui.horizontal_wrapped(|ui| {
            if changes {
                for (mode, label) in [
                    (Mode::Before, t.mode_before),
                    (Mode::Changes, t.mode_changes),
                    (Mode::After, t.mode_after),
                ] {
                    ui.selectable_value(&mut sight.mode, mode, label);
                }
                ui.separator();
            }
            if has_model && ui.button(t.fit).clicked() {
                act = Some(Act::Fit);
            }
            let note = match (has_model, changes, &problem) {
                (false, _, _) => t.intake_3d_nothing.to_string(),
                (true, true, _) => t.intake_3d_colors.to_string(),
                (true, false, Some(_)) => format!(
                    "{}: {} «{a}», {} «{b}».",
                    t.intake_3d_apart, t.intake_3d_left, t.intake_3d_right
                ),
                (true, false, None) => t.intake_3d_same.to_string(),
            };
            ui.label(note);
        });
        if let Some(why) = &problem {
            ui.add(egui::Label::new(RichText::new(why).weak()).wrap());
        }
        if behind {
            let warn = ui.visuals().warn_fg_color;
            let text = RichText::new(t.intake_3d_old_step).color(warn);
            ui.add(egui::Label::new(text).wrap());
        }
        let lines = sight.lines();
        // Слова под картинкой: сколько нужно, но не больше трети места.
        let words = ((lines.len().max(1) as f32 + 2.0) * 20.0).min(ui.available_height() * 0.35);
        let height = (ui.available_height() - words).max(120.0);
        let size = egui::vec2(ui.available_width(), height);
        let (mode, look, wheel) = (sight.mode, app.look, app.settings.wheel_reverse);
        ui.allocate_ui(size, |ui| view.show(ui, mode, &look, None, wheel));
        ui.separator();
        ui.label(RichText::new(format!("{}:", t.intake_3d_changed)).strong());
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if lines.is_empty() {
                    ui.label(RichText::new(t.intake_3d_no_words).weak());
                }
                for line in &lines {
                    ui.add(egui::Label::new(format!("•  {line}")).wrap());
                }
            });
    });
    let ctx = ui.ctx().clone();
    match act {
        None => {}
        Some(Act::Fit) => {
            if let Some(view) = app.intake_view.as_mut() {
                view.set_view(&ctx, None);
            }
        }
        // Назад к плану: идущий расчёт больше не нужен.
        Some(Act::Back | Act::Stop) => {
            if let Some(sight) = app.tab.intake.as_mut().and_then(|i| i.sight.take()) {
                sight.stop();
            }
        }
    }
    true
}

/// Идёт расчёт: строка хода и «Остановить».
fn working(ui: &mut egui::Ui, sight: &Sight, act: &mut Option<Act>) {
    let t = texts();
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spinner();
        let line = sight.line.lock().map(|l| l.clone()).unwrap_or_default();
        ui.label(line);
    });
    ui.add_space(4.0);
    ui.add(egui::Label::new(RichText::new(t.intake_3d_hint).weak()).wrap());
    ui.add_space(8.0);
    if ui.button(t.stop).clicked() {
        *act = Some(Act::Stop);
    }
}
