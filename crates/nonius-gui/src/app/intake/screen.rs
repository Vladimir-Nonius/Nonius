//! Экран разбора папки в центре окна: ход изучения, вопрос с кнопками
//! «Да / Нет / Потом», план по папкам проекта, «Разложить» и итог. У вопроса
//! и у детали с версиями — «Показать в 3D» (сам показ — `sight.rs`).

use eframe::egui::{self, RichText};

use super::{Intake, Stage};
use crate::app::App;
use crate::source::{IntakeItem, IntakeOutcome, IntakePlan, IntakeQuestion, IntakeSide};
use crate::texts::texts;

/// Что человек нажал на экране в этом кадре.
enum Act {
    Stop,
    Close,
    Answer(String, bool),
    Later(String),
    AskAgain,
    Apply,
    /// Показать две модели в 3D и слова, что изменилось.
    Look(IntakeSide, IntakeSide, Vec<String>),
}

/// Нарисовать экран; `false` — экран не открыт, центр окна свободен.
pub fn show(app: &mut App, ui: &mut egui::Ui) -> bool {
    if super::sight::show(app, ui) {
        return true;
    }
    let Some(intake) = app.tab.intake.as_ref() else {
        return false;
    };
    let mut act = None;
    egui::Frame::NONE.inner_margin(12).show(ui, |ui| {
        header(ui, intake, &mut act);
        ui.separator();
        match &intake.stage {
            Stage::Studying | Stage::Applying => working(ui, intake, &mut act),
            Stage::Plan => plan(ui, intake, &mut act),
            Stage::Done(outcome) => done(ui, outcome, &mut act),
        }
    });
    let ctx = ui.ctx().clone();
    match act {
        None => {}
        Some(Act::Stop) => intake.stop(),
        Some(Act::Close) => {
            // Идущая работа останавливается, её результат уже не нужен.
            intake.stop();
            app.tab.intake = None;
        }
        Some(Act::Answer(id, answer)) => app.intake_answer(&ctx, id, answer),
        Some(Act::Later(id)) => {
            if let Some(i) = app.tab.intake.as_mut() {
                i.later.push(id);
            }
        }
        Some(Act::AskAgain) => {
            if let Some(i) = app.tab.intake.as_mut() {
                i.later.clear();
            }
        }
        Some(Act::Apply) => app.intake_apply(&ctx),
        Some(Act::Look(before, after, words)) => app.intake_look(&ctx, before, after, words),
    }
    true
}

fn header(ui: &mut egui::Ui, intake: &Intake, act: &mut Option<Act>) {
    let t = texts();
    ui.horizontal(|ui| {
        ui.heading(t.intake_title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Пока раскладывается, закрыть нельзя: сначала «Остановить».
            let free = !matches!(intake.stage, Stage::Applying);
            if ui.add_enabled(free, egui::Button::new(t.close)).clicked() {
                *act = Some(Act::Close);
            }
        });
    });
    ui.label(RichText::new(intake.folder.display().to_string()).weak());
}

/// Изучение или раскладка: строка хода и «Остановить».
fn working(ui: &mut egui::Ui, intake: &Intake, act: &mut Option<Act>) {
    let t = texts();
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(intake.progress().unwrap_or_default());
    });
    ui.add_space(4.0);
    let hint = match intake.stage {
        Stage::Studying => t.intake_studying,
        _ => t.intake_applying_hint,
    };
    ui.label(RichText::new(hint).weak());
    ui.add_space(8.0);
    if intake.stopping() {
        ui.label(t.intake_stopping);
    } else if ui.button(t.stop).clicked() {
        *act = Some(Act::Stop);
    }
}

/// «Деталей — 12, сборок — 2, версий — 20, …».
pub(super) fn summary(plan: &IntakePlan) -> String {
    let t = texts();
    let assemblies = plan.items.iter().filter(|i| i.assembly).count();
    let versions: usize = plan.items.iter().map(|i| i.versions).sum();
    let docs: usize = plan.items.iter().map(|i| i.docs.len()).sum();
    format!(
        "{} — {}, {} — {}, {} — {}, {} — {}, {} — {}",
        t.intake_parts,
        plan.items.len() - assemblies,
        t.intake_assemblies,
        assemblies,
        t.intake_versions_total,
        versions,
        t.intake_docs,
        docs,
        t.intake_other,
        plan.left.len() + plan.other
    )
}

fn plan(ui: &mut egui::Ui, intake: &Intake, act: &mut Option<Act>) {
    let t = texts();
    let plan = &intake.plan;
    ui.label(summary(plan));
    if plan.items.is_empty() {
        ui.add_space(8.0);
        ui.label(t.intake_empty);
        return;
    }
    ui.add_space(6.0);
    if intake.recounting {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(t.intake_recount);
        });
    } else if let Some(q) = intake.question() {
        question(ui, q, intake.questions_left(), act);
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let ready = !intake.recounting;
        let apply = egui::Button::new(RichText::new(t.intake_apply).strong());
        if ui.add_enabled(ready, apply).clicked() {
            *act = Some(Act::Apply);
        }
        if !intake.later.is_empty() {
            ui.label(format!("{}: {}", t.intake_later_count, intake.later.len()));
            if ui.button(t.intake_ask_again).clicked() {
                *act = Some(Act::AskAgain);
            }
        }
    });
    ui.label(RichText::new(t.intake_safe).weak());
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            items(ui, &plan.items, act);
            leftovers(ui, plan);
        });
}

/// Вопрос в рамке: текст, улики, мнение программы и три кнопки.
fn question(ui: &mut egui::Ui, q: &IntakeQuestion, left: usize, act: &mut Option<Act>) {
    let t = texts();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let title = format!("{} ({} {left})", t.intake_question, t.intake_left_count);
        ui.label(RichText::new(title).weak());
        ui.add(egui::Label::new(RichText::new(&q.text).strong()).wrap());
        for fact in &q.facts {
            ui.add(egui::Label::new(format!("    {fact}")).wrap());
        }
        match q.suggested {
            Some(true) => ui.label(RichText::new(t.intake_think_yes).italics()),
            Some(false) => ui.label(RichText::new(t.intake_think_no).italics()),
            None => ui.label(RichText::new(t.intake_think_none).italics()),
        };
        look(ui, &q.sides, &q.changed, act);
        ui.horizontal(|ui| {
            if ui.button(t.intake_yes).clicked() {
                *act = Some(Act::Answer(q.id.clone(), true));
            }
            if ui.button(t.intake_no).clicked() {
                *act = Some(Act::Answer(q.id.clone(), false));
            }
            if ui.button(t.intake_later).clicked() {
                *act = Some(Act::Later(q.id.clone()));
            }
        });
    });
}

/// Строка детали: «деталь «кронштейн», версий: 3  (?)».
pub(super) fn item_line(item: &IntakeItem) -> String {
    let t = texts();
    let kind = if item.assembly {
        t.intake_assembly
    } else {
        t.intake_part
    };
    let mut line = format!("{kind} «{}»", item.name);
    if item.versions > 1 {
        line = format!("{line}, {}: {}", t.intake_versions, item.versions);
    }
    if item.unsure {
        line.push_str("  (?)");
    }
    line
}

/// Кнопки «Показать в 3D» (`sight.rs`): нажатая — в действие экрана.
fn look(ui: &mut egui::Ui, sides: &[IntakeSide], changed: &[Vec<String>], act: &mut Option<Act>) {
    if let Some((before, after, words)) = super::sight::buttons(ui, sides, changed) {
        *act = Some(Act::Look(before, after, words));
    }
}

/// Детали и сборки по папкам проекта, под каждой — «почему» и бумаги.
fn items(ui: &mut egui::Ui, items: &[IntakeItem], act: &mut Option<Act>) {
    let t = texts();
    let warn = ui.visuals().warn_fg_color;
    let mut folder = None;
    for item in items {
        if folder != Some(&item.folder) {
            folder = Some(&item.folder);
            ui.add_space(6.0);
            let title = if item.folder.is_empty() {
                format!("{}:", t.intake_to_root)
            } else {
                format!("{} «{}»:", t.intake_to_folder, item.folder)
            };
            ui.label(RichText::new(title).strong());
        }
        let line = RichText::new(item_line(item));
        ui.label(if item.unsure { line.color(warn) } else { line });
        ui.indent(("intake-why", &item.folder, &item.name), |ui| {
            for why in &item.why {
                ui.add(egui::Label::new(RichText::new(why).weak()).wrap());
            }
            for doc in &item.docs {
                ui.label(RichText::new(format!("+ {doc}")).weak());
            }
            look(ui, &item.sides, &item.changed, act);
        });
    }
    if items.iter().any(|i| i.unsure) {
        ui.add_space(6.0);
        ui.label(RichText::new(t.intake_unsure_note).color(warn));
    }
}

/// Что останется в папке и не попадёт в проект.
fn leftovers(ui: &mut egui::Ui, plan: &IntakePlan) {
    let t = texts();
    if !plan.left.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(format!("{}:", t.intake_stay)).strong());
        for (name, why) in &plan.left {
            ui.label(format!("{name} — {why}"));
        }
    }
    if plan.other > 0 {
        ui.add_space(8.0);
        let text = format!("{}: {}. {}", t.intake_junk, plan.other, t.intake_junk_note);
        ui.label(RichText::new(text).weak());
    }
}

/// «Разложено: 12, версий сохранено: 20».
pub(super) fn outcome_line(outcome: &IntakeOutcome) -> String {
    let t = texts();
    format!(
        "{}: {}, {}: {}",
        t.intake_done, outcome.parts, t.intake_saved_versions, outcome.versions
    )
}

fn done(ui: &mut egui::Ui, outcome: &IntakeOutcome, act: &mut Option<Act>) {
    let t = texts();
    ui.add_space(8.0);
    ui.label(RichText::new(outcome_line(outcome)).strong());
    if outcome.stopped {
        ui.label(t.intake_stopped);
    }
    ui.add_space(6.0);
    if ui.button(t.intake_finish).clicked() {
        *act = Some(Act::Close);
    }
    ui.add_space(6.0);
    let warn = ui.visuals().warn_fg_color;
    let error = ui.visuals().error_fg_color;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (name, why) in &outcome.failed {
                let text = format!("{}: «{name}» — {why}", t.intake_failed);
                ui.add(egui::Label::new(RichText::new(text).color(error)).wrap());
            }
            for w in &outcome.warnings {
                let text = format!("{}: {w}", t.intake_warning);
                ui.add(egui::Label::new(RichText::new(text).color(warn)).wrap());
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_for_people() {
        let part = IntakeItem {
            name: "кронштейн".into(),
            versions: 3,
            unsure: true,
            docs: vec!["кронштейн.pdf".into()],
            ..IntakeItem::default()
        };
        let frame = IntakeItem {
            name: "рама".into(),
            assembly: true,
            versions: 1,
            ..IntakeItem::default()
        };
        assert_eq!(item_line(&part), "деталь «кронштейн», версий: 3  (?)");
        assert_eq!(item_line(&frame), "сборка «рама»");
        let plan = IntakePlan {
            items: vec![part, frame],
            left: vec![("ТЗ.docx".into(), "непонятно, к какой детали".into())],
            other: 4,
            ..IntakePlan::default()
        };
        assert_eq!(
            summary(&plan),
            "деталей — 1, сборок — 1, версий — 4, чертежей и бумаг при них — 1, прочих файлов — 5"
        );
        let outcome = IntakeOutcome {
            parts: 2,
            versions: 4,
            ..IntakeOutcome::default()
        };
        assert_eq!(outcome_line(&outcome), "Разложено: 2, версий сохранено: 4");
    }
}
