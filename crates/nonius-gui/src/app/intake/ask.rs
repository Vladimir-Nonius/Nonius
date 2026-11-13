//! Рамка вопроса на экране разбора: текст, улики, мнение программы,
//! «Показать в 3D» и кнопки «Да / Нет / Потом». Пока идёт уточнение через
//! SolidWorks, спорный вопрос помечен «уточняю…».

use eframe::egui::{self, RichText};

use super::screen::{look, Act};
use crate::source::IntakeQuestion;
use crate::texts::texts;

/// Вопрос в рамке: текст, улики, мнение программы и три кнопки.
pub(super) fn question(
    ui: &mut egui::Ui,
    q: &IntakeQuestion,
    left: usize,
    refining: bool,
) -> Option<Act> {
    let mut act = None;
    let t = texts();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let mut title = format!("{} ({} {left})", t.intake_question, t.intake_left_count);
        if refining && q.doubtful {
            title = format!("{title} — {}", t.intake_refining_mark);
        }
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
        look(ui, &q.sides, &q.changed, &mut act);
        ui.horizontal(|ui| {
            if ui.button(t.intake_yes).clicked() {
                act = Some(Act::Answer(q.id.clone(), true));
            }
            if ui.button(t.intake_no).clicked() {
                act = Some(Act::Answer(q.id.clone(), false));
            }
            if ui.button(t.intake_later).clicked() {
                act = Some(Act::Later(q.id.clone()));
            }
        });
    });
    act
}
