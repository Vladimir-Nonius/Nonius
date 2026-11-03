//! История детали внутри дерева: «Сейчас» (несохранённое), версии с метками
//! «было» и «стало», кнопка «Сохранить версию». Щелчок по версии другой
//! детали выбирает её. Что значит щелчок и правая кнопка — общее с картой снизу.

use eframe::egui::{self, Color32, RichText};

use super::part_actions::old_version;
use super::App;
use crate::source::{PartEntry, Pick, VersionEntry};
use crate::texts::texts;

/// Что попросили в истории или на карте.
pub enum Act {
    Show((Pick, Pick)),
    /// Выбрать другую деталь и показать у неё эту пару.
    Open(String, (Pick, Pick)),
    Save,
    /// Вернуть деталь к версии.
    Restore(String, u32),
    /// Выгрузить файлы версии детали.
    Export(String, u32),
}

/// Метка версии в выбранной паре.
#[derive(Debug, Clone, Copy)]
pub struct Mark {
    pub text: &'static str,
    pub color: Color32,
    pub fill: Color32,
}

/// «Было» или «Стало», если версия в паре.
pub fn mark(me: Pick, picks: (Pick, Pick)) -> Option<Mark> {
    let t = texts();
    if picks.0 == me && picks.1 == me {
        Some(Mark {
            text: t.badge_shown,
            color: Color32::from_rgb(60, 110, 190),
            fill: Color32::from_rgb(232, 239, 250),
        })
    } else if picks.1 == me {
        Some(Mark {
            text: t.badge_after,
            color: Color32::from_rgb(31, 158, 70),
            fill: Color32::from_rgb(230, 245, 234),
        })
    } else if picks.0 == me {
        Some(Mark {
            text: t.badge_before,
            color: Color32::from_rgb(210, 59, 59),
            fill: Color32::from_rgb(251, 232, 232),
        })
    } else {
        None
    }
}

/// Щелчок по версии `i` (новые первыми): она «Стало», «Было» — предыдущая.
pub fn version_pair(versions: &[VersionEntry], i: usize) -> (Pick, Pick) {
    let me = Pick::Version(versions[i].number);
    let before = versions.get(i + 1).map_or(me, |p| Pick::Version(p.number));
    (before, me)
}

/// Щелчок по «Сейчас»: сравнить с последней версией.
pub fn now_pair(versions: &[VersionEntry]) -> Option<(Pick, Pick)> {
    let latest = versions.first()?;
    Some((Pick::Version(latest.number), Pick::Current))
}

/// Правая кнопка по версии детали `part`. «Было» и «Стало» — только у
/// выбранной (`picks` есть), остальное — у любой. `latest` вернуть незачем.
pub fn version_menu(
    r: &egui::Response,
    part: &str,
    me: Pick,
    picks: Option<(Pick, Pick)>,
    latest: Option<u32>,
    act: &mut Option<Act>,
) {
    let t = texts();
    r.context_menu(|ui| {
        if let Some(picks) = picks {
            if ui.button(t.make_before).clicked() {
                *act = Some(Act::Show((me, picks.1)));
                ui.close();
            }
            if ui.button(t.make_after).clicked() {
                *act = Some(Act::Show((picks.0, me)));
                ui.close();
            }
            ui.separator();
        }
        let old = old_version(me, latest);
        if ui
            .add_enabled(old.is_some(), egui::Button::new(t.restore_version))
            .clicked()
        {
            *act = old.map(|n| Act::Restore(part.to_string(), n));
            ui.close();
        }
        if let (Pick::Version(n), true) = (me, ui.button(t.export_version).clicked()) {
            *act = Some(Act::Export(part.to_string(), n));
            ui.close();
        }
    });
}

/// Правая кнопка по «Сейчас»; «Было» — только сохранённая версия.
pub fn now_menu(r: &egui::Response, picks: (Pick, Pick), act: &mut Option<Act>) {
    let t = texts();
    r.context_menu(|ui| {
        if ui.button(t.make_after).clicked() {
            *act = Some(Act::Show((picks.0, Pick::Current)));
            ui.close();
        }
    });
}

impl App {
    /// Выполнить то, что попросили в истории или на карте.
    pub(super) fn apply(&mut self, ctx: &egui::Context, act: Option<Act>) {
        match act {
            Some(Act::Show(p)) if p != self.picks || self.outside.is_some() => {
                self.show_picks(ctx, p)
            }
            Some(Act::Open(part, picks)) => {
                self.want = Some((part.clone(), Some(picks)));
                self.select_part(ctx, &part);
            }
            Some(Act::Save) => self.start_save(ctx),
            Some(Act::Restore(part, n)) => self.ask_restore_of(&part, n),
            Some(Act::Export(part, n)) => self.export_of(ctx, part, n),
            _ => {}
        }
    }

    /// Есть несохранённое у выбранной детали.
    pub(super) fn part_dirty(&self) -> bool {
        self.parts
            .iter()
            .any(|p| Some(&p.name) == self.part.as_ref() && p.dirty)
    }
}

/// Нарисовать историю детали `part`.
pub fn show(app: &mut App, ui: &mut egui::Ui, part: &PartEntry) {
    let t = texts();
    let chosen = app.part.as_ref() == Some(&part.name);
    let empty = Vec::new();
    let versions = if chosen {
        &app.versions
    } else {
        app.listed.get(&part.name).unwrap_or(&empty)
    };
    let picks = chosen.then_some(app.picks);
    let latest = versions.first().map(|v| v.number);
    let go = |pair: (Pick, Pick)| {
        if chosen {
            Act::Show(pair)
        } else {
            Act::Open(part.name.clone(), pair)
        }
    };
    let mut act = None;
    if part.dirty {
        let r = row(ui, Pick::Current, picks, [t.now, t.not_saved, ""]);
        if r.clicked() {
            let pair = now_pair(versions).unwrap_or((Pick::Current, Pick::Current));
            act = Some(go(pair));
        }
        if let (Some(picks), Some(_)) = (picks, latest) {
            now_menu(&r, picks, &mut act);
        }
        if chosen {
            let label = match latest {
                Some(n) => format!("{} v{}…", t.save_version, n + 1),
                None => format!("{}…", t.save_first),
            };
            let busy = app.save.is_some();
            if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                act = Some(Act::Save);
            }
        }
        ui.add_space(4.0);
    }
    for (i, v) in versions.iter().enumerate() {
        let me = Pick::Version(v.number);
        let title = format!("v{}", v.number);
        let sub = format!("{} · {}", v.date, v.author);
        let r = row(ui, me, picks, [&title, &sub, &v.summary]);
        if r.clicked() {
            act = Some(go(version_pair(versions, i)));
        }
        version_menu(&r, &part.name, me, picks, latest, &mut act);
    }
    let ctx = ui.ctx().clone();
    app.apply(&ctx, act);
}

/// Строка истории: рамка цвета роли («было» — красноватая, «стало» —
/// зеленоватая): номер с меткой справа, под ним дата и автор, потом
/// описание. Щелчок — по всей рамке.
fn row(
    ui: &mut egui::Ui,
    me: Pick,
    picks: Option<(Pick, Pick)>,
    [title, sub, summary]: [&str; 3],
) -> egui::Response {
    let badge = picks.and_then(|p| mark(me, p));
    let inner = egui::Frame::NONE
        .fill(badge.map_or(Color32::TRANSPARENT, |m| m.fill))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            // Щелчок — по рамке, а не выделение текста.
            ui.style_mut().interaction.selectable_labels = false;
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).strong());
                if let Some(m) = badge {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(m.text)
                                .small()
                                .color(Color32::WHITE)
                                .background_color(m.color),
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
