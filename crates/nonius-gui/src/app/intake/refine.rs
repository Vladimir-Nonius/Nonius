//! Уточнение разбора через SolidWorks в фоне. План показывается сразу — по
//! форме, снятой прямо из файлов (быстрый проход); потом, если это
//! разрешено настройкой «Уточнять разбор папки через SolidWorks», идёт
//! точный проход: связи сборок и чертежей, точная форма спорных деталей,
//! слова «что изменилось». Пока он идёт, спорные строки плана помечены
//! «уточняю…», на вопросы можно отвечать, а «Разложить» ждёт. Закончился —
//! план пересчитывается с ответами человека.
//!
//! «Остановить» или снятая галочка бросают уточнение: план остаётся
//! беглым, а уточнить можно снова — кнопкой «Уточнить» или галочкой; уже
//! посчитанное программа помнит.

use std::sync::atomic::Ordering;

use eframe::egui::{self, RichText};

use super::{Intake, Stage, Steps};
use crate::app::jobs::Done;
use crate::app::App;
use crate::source::IntakePlan;
use crate::texts::texts;

/// Что человек сделал в полосе уточнения.
pub(super) enum Pick {
    /// Галочка «Уточнять через SolidWorks»: новое значение.
    Switch(bool),
    /// «Остановить» уточнение.
    Stop,
    /// «Уточнить» снова после остановки.
    Again,
}

/// Полоса над планом: идёт уточнение — строка и «Остановить»; план беглый
/// и уточнить можно — пояснение. Галочка — всегда, пока уточнять есть что.
pub(super) fn bar(ui: &mut egui::Ui, intake: &Intake, on: bool) -> Option<Pick> {
    let t = texts();
    if !intake.refining && !intake.plan.refine {
        return None;
    }
    let mut pick = None;
    ui.horizontal(|ui| {
        let mut want = on;
        if ui
            .checkbox(&mut want, t.intake_refine)
            .on_hover_text(t.intake_refine_hint)
            .changed()
        {
            pick = Some(Pick::Switch(want));
        }
        if intake.refining {
            ui.spinner();
            if intake.stopping() {
                ui.label(t.intake_stopping);
            } else if ui.button(t.stop).clicked() {
                pick = Some(Pick::Stop);
            }
        } else if on && ui.button(t.intake_refine_again).clicked() {
            pick = Some(Pick::Again);
        }
    });
    let note = if intake.refining {
        t.intake_refining
    } else if on {
        t.intake_refined_partly
    } else {
        t.intake_rough_note
    };
    ui.add(egui::Label::new(RichText::new(note).weak()).wrap());
    pick
}

impl App {
    /// Папка изучена быстрым проходом: план — на экран, уточнение — в фон.
    pub(in crate::app) fn intake_studied(
        &mut self,
        ctx: &egui::Context,
        run: u64,
        result: anyhow::Result<IntakePlan>,
    ) {
        self.intake_planned(run, result);
        self.intake_refine_start(ctx);
    }

    /// Начать точный проход, если он нужен и разрешён настройкой.
    fn intake_refine_start(&mut self, ctx: &egui::Context) {
        if !self.settings.intake_refine {
            return;
        }
        let (Some(project), Some(intake)) = (self.tab.project.clone(), self.tab.intake.as_mut())
        else {
            return;
        };
        if !matches!(intake.stage, Stage::Plan) || intake.refining || !intake.plan.refine {
            return;
        }
        intake.refining = true;
        intake.stop.store(false, Ordering::Relaxed);
        if let Ok(mut l) = intake.line.lock() {
            *l = texts().intake_refining_start.to_string();
        }
        let steps = Steps {
            ctx: ctx.clone(),
            line: intake.line.clone(),
            stop: intake.stop.clone(),
        };
        let run = intake.run;
        self.tab.jobs.spawn(ctx, move |_| {
            Done::IntakeRefined(run, project.intake_refine(&steps))
        });
    }

    /// Точный проход закончился: план — заново, с ответами человека.
    pub(in crate::app) fn intake_refined(
        &mut self,
        ctx: &egui::Context,
        run: u64,
        result: anyhow::Result<()>,
    ) {
        let Some(intake) = self.intake_of(run) else {
            return;
        };
        intake.refining = false;
        intake.stop.store(false, Ordering::Relaxed);
        if let Err(e) = result {
            self.tab.message = Some(format!("{}: {e:#}", texts().error));
            return;
        }
        self.intake_recount(ctx);
    }

    /// Человек нажал в полосе уточнения.
    pub(super) fn intake_refine_pick(&mut self, ctx: &egui::Context, pick: Pick) {
        match pick {
            Pick::Switch(on) => {
                self.settings.intake_refine = on;
                self.settings.save();
                if on {
                    self.intake_refine_start(ctx);
                } else if let Some(i) = self.tab.intake.as_ref().filter(|i| i.refining) {
                    i.stop.store(true, Ordering::Relaxed);
                }
            }
            Pick::Stop => {
                if let Some(i) = self.tab.intake.as_ref() {
                    i.stop.store(true, Ordering::Relaxed);
                }
            }
            Pick::Again => self.intake_refine_start(ctx),
        }
    }
}
