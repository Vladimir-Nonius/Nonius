//! История выбранной детали в левой панели: «Сейчас» (несохранённое),
//! версии с метками «было» и «стало», кнопка «Сохранить версию».

use eframe::egui::{self, Color32, RichText};

use super::App;
use crate::source::Pick;
use crate::texts::texts;

/// Что попросили в истории.
enum Act {
    Show((Pick, Pick)),
    Save,
}

/// Нарисовать историю под списком деталей.
pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let t = texts();
    let Some(part) = app.part.clone() else {
        return;
    };
    let dirty = app.parts.iter().any(|p| p.name == part && p.dirty);
    ui.add_space(10.0);
    ui.label(RichText::new(format!("{} «{part}»", t.history)).strong());
    ui.separator();
    let picks = app.picks;
    let latest = app.versions.first().map(|v| v.number);
    let mut act = None;
    egui::ScrollArea::vertical()
        .id_salt("history")
        .show(ui, |ui| {
            if dirty {
                let r = row(ui, Pick::Current, picks, [t.now, t.not_saved, ""]);
                if let (true, Some(n)) = (r.clicked(), latest) {
                    act = Some(Act::Show((Pick::Version(n), Pick::Current)));
                }
                r.context_menu(|ui| {
                    if latest.is_some() && ui.button(t.make_after).clicked() {
                        act = Some(Act::Show((picks.0, Pick::Current)));
                    }
                });
                let label = match latest {
                    Some(n) => format!("{} v{}…", t.save_version, n + 1),
                    None => format!("{}…", t.save_first),
                };
                let busy = app.save.is_some();
                if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                    act = Some(Act::Save);
                }
                ui.add_space(4.0);
            }
            for (i, v) in app.versions.iter().enumerate() {
                let me = Pick::Version(v.number);
                let title = format!("v{}", v.number);
                let sub = format!("{} · {}", v.date, v.author);
                let r = row(ui, me, picks, [&title, &sub, &v.summary]);
                if r.clicked() {
                    let before = app
                        .versions
                        .get(i + 1)
                        .map_or(me, |p| Pick::Version(p.number));
                    act = Some(Act::Show((before, me)));
                }
                r.context_menu(|ui| {
                    if ui.button(t.make_before).clicked() {
                        act = Some(Act::Show((me, picks.1)));
                        ui.close();
                    }
                    if ui.button(t.make_after).clicked() {
                        act = Some(Act::Show((picks.0, me)));
                        ui.close();
                    }
                });
            }
            if !app.versions.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(t.history_hint).weak());
            }
        });
    match act {
        Some(Act::Show(p)) if p != app.picks => {
            let ctx = ui.ctx().clone();
            app.show_picks(&ctx, p);
        }
        Some(Act::Save) => {
            let ctx = ui.ctx().clone();
            app.start_save(&ctx);
        }
        _ => {}
    }
}

/// Строка истории: рамка цвета роли («было» — красноватая, «стало» —
/// зеленоватая): номер с меткой справа, под ним дата и автор, потом
/// описание. Щелчок — по всей рамке.
fn row(
    ui: &mut egui::Ui,
    me: Pick,
    picks: (Pick, Pick),
    [title, sub, summary]: [&str; 3],
) -> egui::Response {
    let t = texts();
    let (fill, badge) = if picks.1 == me {
        (
            Color32::from_rgb(230, 245, 234),
            Some((t.badge_after, Color32::from_rgb(31, 158, 70))),
        )
    } else if picks.0 == me {
        (
            Color32::from_rgb(251, 232, 232),
            Some((t.badge_before, Color32::from_rgb(210, 59, 59))),
        )
    } else {
        (Color32::TRANSPARENT, None)
    };
    let inner = egui::Frame::NONE
        .fill(fill)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            // Щелчок — по рамке, а не выделение текста.
            ui.style_mut().interaction.selectable_labels = false;
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).strong());
                if let Some((text, color)) = badge {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(text)
                                .small()
                                .color(Color32::WHITE)
                                .background_color(color),
                        );
                    });
                }
            });
            ui.label(RichText::new(sub).weak());
            if !summary.is_empty() {
                ui.add(egui::Label::new(summary).wrap());
            }
        });
    ui.interact(
        inner.response.rect,
        ui.id().with(("history-row", me)),
        egui::Sense::click(),
    )
}
