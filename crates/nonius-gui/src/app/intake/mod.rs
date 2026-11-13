//! «Разобрать папку»: программа изучает папку с моделями в фоне, показывает
//! план (что станет деталями и версиями и почему), задаёт вопросы по одному
//! и по кнопке раскладывает копии файлов в проект. Здесь — состояние и
//! действия; сам экран — `screen.rs`.
//!
//! Изучение идёт минуты (форму деталей даёт SolidWorks), поэтому у него своя
//! строка хода и «Остановить»: план тогда строится из того, что успели.
//!
//! «Показать в 3D» у вопроса и у детали с версиями — `sight.rs`: две модели
//! из папки в 3D, тоже в фоне со своей строкой хода.

mod screen;
mod sight;

pub use screen::show;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;

use super::jobs::Done;
use super::App;
use crate::source::{IntakeOutcome, IntakePlan, IntakeQuestion, Progress};
use crate::texts::texts;

/// Что сейчас на экране разбора.
pub(super) enum Stage {
    /// Папка изучается.
    Studying,
    /// План показан: вопросы и «Разложить».
    Plan,
    /// Файлы раскладываются.
    Applying,
    /// Разложено: итог.
    Done(IntakeOutcome),
}

/// Открытый экран разбора папки.
pub(in crate::app) struct Intake {
    /// Номер разбора: результат прошлого, закрытого, сюда не попадёт.
    run: u64,
    folder: PathBuf,
    stage: Stage,
    plan: IntakePlan,
    /// Ответы человека: `id` вопроса и «да»/«нет».
    answers: Vec<(String, bool)>,
    /// Вопросы, отложенные кнопкой «Потом».
    later: Vec<String>,
    /// План пересчитывается после ответа: кнопки ждут.
    recounting: bool,
    stop: Arc<AtomicBool>,
    /// Своя строка хода: сравнение её не трогает.
    line: Arc<Mutex<String>>,
    /// Открытое «Показать в 3D» и счётчик показов.
    sight: Option<sight::Sight>,
    sights: u64,
}

impl Intake {
    /// Идёт ли долгая работа: изучение или раскладка.
    fn working(&self) -> bool {
        matches!(self.stage, Stage::Studying | Stage::Applying)
    }

    /// Строка хода, пока идёт долгая работа, — для строки состояния.
    pub fn progress(&self) -> Option<String> {
        self.working()
            .then(|| self.line.lock().map(|l| l.clone()).unwrap_or_default())
    }

    /// Попросить остановиться: изучение отдаст план из того, что успело,
    /// раскладка закончит текущую деталь, сравнение в 3D бросит расчёт.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(sight) = &self.sight {
            sight.stop();
        }
    }

    fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Вопрос, который пора задать: первый без ответа и не отложенный.
    fn question(&self) -> Option<&IntakeQuestion> {
        self.plan
            .questions
            .iter()
            .find(|q| !self.later.contains(&q.id))
    }

    /// Сколько вопросов ещё не отложено.
    fn questions_left(&self) -> usize {
        let open = |q: &&IntakeQuestion| !self.later.contains(&q.id);
        self.plan.questions.iter().filter(open).count()
    }
}

/// Ход работы → своя строка экрана; «Остановить» → флажок.
struct Steps {
    ctx: egui::Context,
    line: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
}

impl Progress for Steps {
    fn step(&self, what: &str) {
        if let Ok(mut l) = self.line.lock() {
            *l = what.to_string();
        }
        self.ctx.request_repaint();
    }

    fn cancelled(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

impl App {
    /// «Разобрать папку…»: спросить папку и начать изучать её в фоне.
    pub(super) fn start_intake(&mut self, ctx: &egui::Context) {
        let t = texts();
        if self.tab.intake.as_ref().is_some_and(Intake::working) {
            self.tab.notice = Some(t.intake_busy.to_string());
            return;
        }
        let Some(folder) = self.chooser.folder(&self.path_input) else {
            return;
        };
        let Some(project) = self.tab.project.clone() else {
            return;
        };
        self.tab.intake_runs += 1;
        let run = self.tab.intake_runs;
        let stop = Arc::new(AtomicBool::new(false));
        let line = Arc::new(Mutex::new(t.intake_starting.to_string()));
        self.tab.intake = Some(Intake {
            run,
            folder: folder.clone(),
            stage: Stage::Studying,
            plan: IntakePlan::default(),
            answers: Vec::new(),
            later: Vec::new(),
            recounting: false,
            stop: stop.clone(),
            line: line.clone(),
            sight: None,
            sights: 0,
        });
        let steps = Steps {
            ctx: ctx.clone(),
            line,
            stop,
        };
        self.tab.jobs.spawn(ctx, move |_| {
            Done::IntakeStudied(run, project.intake_study(&folder, &steps))
        });
    }

    /// Экран этого разбора ещё открыт?
    fn intake_of(&mut self, run: u64) -> Option<&mut Intake> {
        self.tab.intake.as_mut().filter(|i| i.run == run)
    }

    /// Папка изучена (или план пересчитан после ответа).
    pub(super) fn intake_planned(&mut self, run: u64, result: anyhow::Result<IntakePlan>) {
        let Some(intake) = self.intake_of(run) else {
            return;
        };
        intake.recounting = false;
        match result {
            Ok(plan) => {
                intake.plan = plan;
                intake.stage = Stage::Plan;
                // «Остановить» относилось к изучению, а не к будущей раскладке.
                intake.stop.store(false, Ordering::Relaxed);
            }
            // Изучить не вышло — показывать нечего; пересчёт не вышел —
            // прежний план остаётся.
            Err(e) => {
                if matches!(intake.stage, Stage::Studying) {
                    self.tab.intake = None;
                }
                self.tab.message = Some(format!("{}: {e:#}", texts().error));
            }
        }
    }

    /// Ответ на вопрос: «да» или «нет». План пересчитывается сразу.
    fn intake_answer(&mut self, ctx: &egui::Context, id: String, answer: bool) {
        let (Some(project), Some(intake)) = (self.tab.project.clone(), self.tab.intake.as_mut())
        else {
            return;
        };
        intake.answers.push((id, answer));
        intake.recounting = true;
        let (run, answers) = (intake.run, intake.answers.clone());
        self.tab.jobs.spawn(ctx, move |_| {
            Done::IntakePlanned(run, project.intake_plan(&answers))
        });
    }

    /// «Разложить»: копии файлов в проект, версии в историю.
    fn intake_apply(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(intake)) = (self.tab.project.clone(), self.tab.intake.as_mut())
        else {
            return;
        };
        intake.stage = Stage::Applying;
        intake.stop.store(false, Ordering::Relaxed);
        if let Ok(mut l) = intake.line.lock() {
            *l = texts().intake_applying.to_string();
        }
        let steps = Steps {
            ctx: ctx.clone(),
            line: intake.line.clone(),
            stop: intake.stop.clone(),
        };
        let (run, answers) = (intake.run, intake.answers.clone());
        self.tab.jobs.spawn(ctx, move |_| {
            Done::IntakeApplied(run, project.intake_apply(&answers, &steps))
        });
    }

    /// Раскладка закончилась: итог на экран, дерево проекта — заново.
    pub(super) fn intake_applied(
        &mut self,
        ctx: &egui::Context,
        run: u64,
        result: anyhow::Result<IntakeOutcome>,
    ) {
        match result {
            Ok(outcome) => {
                if let Some(intake) = self.intake_of(run) {
                    intake.stage = Stage::Done(outcome);
                }
            }
            Err(e) => {
                if let Some(intake) = self.intake_of(run) {
                    intake.stage = Stage::Plan;
                }
                self.tab.message = Some(format!("{}: {e:#}", texts().error));
            }
        }
        // Даже неудачная раскладка могла успеть положить часть деталей.
        self.reload(ctx);
    }
}

impl super::project_tab::ProjectTab {
    /// Фоновая работа упала: экран разбора не ждёт её вечно.
    pub(super) fn intake_crashed(&mut self) {
        // Сравнение в 3D, которое ещё считалось, закрывается: снова план.
        if let Some(i) = self.intake.as_mut() {
            if i.sight.as_ref().is_some_and(sight::Sight::waiting) {
                i.sight = None;
            }
        }
        match self.intake.as_mut().map(|i| (i.working(), &mut i.stage)) {
            Some((true, stage @ Stage::Applying)) => *stage = Stage::Plan,
            Some((true, _)) => self.intake = None,
            _ => {
                if let Some(i) = self.intake.as_mut() {
                    i.recounting = false;
                }
            }
        }
    }
}
