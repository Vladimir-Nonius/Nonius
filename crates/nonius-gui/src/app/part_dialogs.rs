//! Окошки действий над деталью: спросить подтверждение или новое имя, показать
//! причину отказа, список удалённых деталей.

use eframe::egui::{self, RichText};

use super::App;
use crate::source::DeletedEntry;
use crate::texts::texts;

/// Открытое окошко.
pub enum Dialog {
    /// Новое имя детали.
    Rename(Rename),
    /// Удалить деталь?
    Delete(Confirm),
    /// Вернуть деталь к версии?
    Restore(Confirm, u32),
    /// Убрать файл из папки детали?
    RemoveFile(Confirm, String),
    /// Список удалённых деталей; `None` — ещё читается.
    Deleted(Option<Vec<DeletedEntry>>, Confirm),
}

/// Окошко-вопрос про деталь.
#[derive(Debug, Default)]
pub struct Confirm {
    /// Деталь.
    pub part: String,
    /// Работа идёт, кнопки ждут.
    pub busy: bool,
    /// Почему не получилось.
    pub error: Option<String>,
}

/// Окошко «Переименовать».
#[derive(Debug, Default)]
pub struct Rename {
    /// Как и в других окошках.
    pub base: Confirm,
    /// Новое имя.
    pub name: String,
    pub focused: bool,
}

impl Dialog {
    /// Работа не удалась: окошко остаётся и говорит почему.
    pub fn failed(&mut self, error: String) {
        let base = match self {
            Dialog::Rename(d) => {
                d.focused = false;
                &mut d.base
            }
            Dialog::Delete(c)
            | Dialog::Restore(c, _)
            | Dialog::RemoveFile(c, _)
            | Dialog::Deleted(_, c) => c,
        };
        base.busy = false;
        base.error = Some(error);
    }

    pub(super) fn base(&mut self) -> &mut Confirm {
        match self {
            Dialog::Rename(d) => &mut d.base,
            Dialog::Delete(c)
            | Dialog::Restore(c, _)
            | Dialog::RemoveFile(c, _)
            | Dialog::Deleted(_, c) => c,
        }
    }
}

/// Что нажали в окошке.
#[derive(Debug, PartialEq, Eq)]
enum Pressed {
    Yes,
    No,
}

/// Вопрос с кнопками «да» и «Отмена». `yes` — `None`: сделать нельзя или
/// нечего, тогда окошко только «Закрыть».
fn question(
    ctx: &egui::Context,
    title: &str,
    base: &Confirm,
    yes: Option<&str>,
    body: impl FnOnce(&mut egui::Ui),
) -> Option<Pressed> {
    let t = texts();
    let mut pressed = None;
    let modal = egui::Modal::new(egui::Id::new(("part-question", title))).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.heading(title);
        ui.add_space(6.0);
        body(ui);
        if let Some(e) = &base.error {
            ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
        }
        ui.add_space(6.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(label) = yes {
                let button = egui::Button::new(label);
                if ui.add_enabled(!base.busy, button).clicked() {
                    pressed = Some(Pressed::Yes);
                }
            }
            let no = if yes.is_some() { t.cancel } else { t.close };
            if ui.add_enabled(!base.busy, egui::Button::new(no)).clicked() {
                pressed = Some(Pressed::No);
            }
        });
    });
    if modal.should_close() && !base.busy {
        pressed = pressed.or(Some(Pressed::No));
    }
    pressed
}

/// Нарисовать открытое окошко.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let dirty = |part: &str| app.tab.parts.iter().any(|p| p.name == part && p.dirty);
    let saved = |part: &str| {
        let entry = app.tab.parts.iter().find(|p| p.name == part);
        entry.is_some_and(|p| p.latest.is_some())
    };
    let pressed = match app.tab.dialog.as_mut() {
        None => return,
        Some(Dialog::Delete(base)) => {
            let title = format!("{} «{}»?", t.delete_title, base.part);
            let unsaved = dirty(&base.part);
            question(ctx, &title, base, Some(t.delete), |ui| {
                ui.add(egui::Label::new(t.delete_why).wrap());
                if unsaved {
                    ui.add(egui::Label::new(t.delete_unsaved).wrap());
                }
            })
        }
        Some(Dialog::RemoveFile(base, path)) => {
            let title = format!("{} {path}?", t.remove_file_title);
            question(ctx, &title, base, Some(t.remove), |ui| {
                ui.add(egui::Label::new(t.remove_file_why).wrap());
            })
        }
        Some(Dialog::Restore(base, version)) => {
            let title = format!("{} «{}» к v{version}?", t.restore_title, base.part);
            let unsaved = dirty(&base.part);
            let yes = (!unsaved).then_some(t.restore);
            question(ctx, &title, base, yes, |ui| {
                let text = if unsaved {
                    t.restore_unsaved
                } else {
                    t.restore_why
                };
                ui.add(egui::Label::new(text).wrap());
            })
        }
        Some(Dialog::Rename(d)) => {
            let title = format!("{} «{}»", t.rename_title, d.base.part);
            let blocked = dirty(&d.base.part) && saved(&d.base.part);
            let ready = !blocked && !d.name.trim().is_empty();
            let mut enter = false;
            let (name, focused, base) = (&mut d.name, &mut d.focused, &d.base);
            let answer = question(ctx, &title, base, ready.then_some(t.rename), |ui| {
                ui.label(t.part_name);
                let chars = name.chars().count();
                let mut output = egui::TextEdit::singleline(name)
                    .desired_width(f32::INFINITY)
                    .show(ui);
                let edit = output.response.clone();
                if !*focused {
                    // Старое имя выделено: можно сразу печатать новое.
                    edit.request_focus();
                    let all = egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(chars),
                    );
                    output.state.cursor.set_char_range(Some(all));
                    output.state.store(ui.ctx(), edit.id);
                    *focused = true;
                }
                enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let note = if blocked {
                    t.rename_unsaved
                } else {
                    t.rename_why
                };
                ui.add(egui::Label::new(note).wrap());
            });
            if enter && ready {
                Some(Pressed::Yes)
            } else {
                answer
            }
        }
        Some(Dialog::Deleted(list, base)) => {
            let mut back = None;
            let busy = base.busy;
            let answer = question(ctx, t.deleted_title, base, None, |ui| {
                deleted_list(ui, list.as_deref(), busy, &mut back);
            });
            if let Some(name) = back {
                app.recover(ctx, name);
                return;
            }
            answer
        }
    };
    match pressed {
        Some(Pressed::No) => app.tab.dialog = None,
        Some(Pressed::Yes) => app.confirmed(ctx),
        None => {}
    }
}

fn deleted_list(
    ui: &mut egui::Ui,
    list: Option<&[DeletedEntry]>,
    busy: bool,
    back: &mut Option<String>,
) {
    let t = texts();
    let Some(list) = list else {
        ui.spinner();
        return;
    };
    if list.is_empty() {
        ui.label(RichText::new(t.nothing_deleted).weak());
    }
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .show(ui, |ui| {
            for d in list {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&d.name).strong());
                    ui.label(RichText::new(format!("{} · {}", d.date, d.author)).weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = format!("{} «{}»", t.recover, d.name);
                        if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                            *back = Some(d.name.clone());
                        }
                    });
                });
            }
        });
}
