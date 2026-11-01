//! Что можно сделать с деталью: переименовать, удалить, вернуть к версии,
//! выгрузить, сравнить с файлом. Окошки спрашивают подтверждение и показывают
//! причину отказа, сама работа идёт в фоне.

use eframe::egui::{self, RichText};

use super::jobs::Done;
use super::{pick_label, App};
use crate::source::{DeletedEntry, Other, Pick};
use crate::texts::texts;

/// Открытое окошко.
pub enum Dialog {
    /// Новое имя детали.
    Rename(Rename),
    /// Удалить деталь?
    Delete(Confirm),
    /// Вернуть деталь к версии?
    Restore(Confirm, u32),
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
    focused: bool,
}

impl Dialog {
    /// Работа не удалась: окошко остаётся и говорит почему.
    pub fn failed(&mut self, error: String) {
        let base = match self {
            Dialog::Rename(d) => {
                d.focused = false;
                &mut d.base
            }
            Dialog::Delete(c) | Dialog::Restore(c, _) | Dialog::Deleted(_, c) => c,
        };
        base.busy = false;
        base.error = Some(error);
    }

    fn base(&mut self) -> &mut Confirm {
        match self {
            Dialog::Rename(d) => &mut d.base,
            Dialog::Delete(c) | Dialog::Restore(c, _) | Dialog::Deleted(_, c) => c,
        }
    }
}

/// Что нажали в окошке.
#[derive(Debug, PartialEq, Eq)]
enum Pressed {
    Yes,
    No,
}

/// Вопрос с кнопками «да» и «Отмена». `yes` — `None`: сделать нельзя.
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
            if ui
                .add_enabled(!base.busy, egui::Button::new(t.cancel))
                .clicked()
            {
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
    let dirty = |part: &str| app.parts.iter().any(|p| p.name == part && p.dirty);
    let saved = |part: &str| {
        let entry = app.parts.iter().find(|p| p.name == part);
        entry.is_some_and(|p| p.latest.is_some())
    };
    let pressed = match app.dialog.as_mut() {
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
        Some(Pressed::No) => app.dialog = None,
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

impl App {
    /// Спросить новое имя детали.
    pub(super) fn ask_rename(&mut self, part: &str) {
        self.dialog = Some(Dialog::Rename(Rename {
            name: part.to_string(),
            base: confirm(part),
            ..Rename::default()
        }));
    }

    /// Спросить, удалить ли деталь.
    pub(super) fn ask_delete(&mut self, part: &str) {
        self.dialog = Some(Dialog::Delete(confirm(part)));
    }

    /// Спросить, вернуть ли выбранную деталь к версии.
    pub(super) fn ask_restore(&mut self, version: Option<u32>) {
        if let (Some(part), Some(n)) = (self.part.clone(), version) {
            self.dialog = Some(Dialog::Restore(confirm(&part), n));
        }
    }

    /// Показать удалённые детали.
    pub(super) fn show_deleted(&mut self, ctx: &egui::Context) {
        let Some(project) = self.project.clone() else {
            return;
        };
        self.dialog = Some(Dialog::Deleted(None, Confirm::default()));
        self.jobs
            .spawn(ctx, move |_| Done::DeletedList(project.deleted_parts()));
    }

    /// Человек подтвердил вопрос в окошке.
    fn confirmed(&mut self, ctx: &egui::Context) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        dialog.base().busy = true;
        dialog.base().error = None;
        // Папка детали сейчас изменится: идущее сравнение не нужно.
        self.jobs.cancel();
        match self.dialog.as_ref() {
            Some(Dialog::Rename(d)) => {
                let (old, new) = (d.base.part.clone(), d.name.trim().to_string());
                self.jobs.spawn(ctx, move |_| {
                    let done = project.rename_part(&old, &new);
                    Done::PartRenamed(old, new, done)
                });
            }
            Some(Dialog::Delete(c)) => {
                let part = c.part.clone();
                self.jobs.spawn(ctx, move |_| {
                    let done = project.delete_part(&part);
                    Done::PartDeleted(part, done)
                });
            }
            Some(Dialog::Restore(c, version)) => {
                let (part, version) = (c.part.clone(), *version);
                self.jobs.spawn(ctx, move |_| {
                    let done = project.restore_version(&part, version);
                    Done::Restored(part, version, done)
                });
            }
            _ => {}
        }
    }

    /// Вернуть удалённую деталь из списка.
    fn recover(&mut self, ctx: &egui::Context, part: String) {
        let Some(project) = self.project.clone() else {
            return;
        };
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.base().busy = true;
            dialog.base().error = None;
        }
        self.jobs.spawn(ctx, move |_| {
            let done = project.recover_part(&part);
            Done::PartRecovered(part, done)
        });
    }

    /// Выбрать папку и выгрузить в неё файлы версии.
    pub(super) fn export_version(&mut self, ctx: &egui::Context, version: Option<u32>) {
        let (Some(project), Some(part), Some(n)) =
            (self.project.clone(), self.part.clone(), version)
        else {
            return;
        };
        let Some(dest) = self.chooser.folder(&self.path_input) else {
            return;
        };
        self.jobs.spawn(ctx, move |_| {
            let done = project.export_version(&part, n, &dest);
            Done::Exported(part, n, dest, done)
        });
    }

    /// Сравнить показанную версию с файлом или другой деталью. В историю
    /// ничего не попадает.
    pub(super) fn compare_with(&mut self, ctx: &egui::Context, other: Other) {
        let (Some(project), Some(part)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        let pick = self.picks.1;
        let label = match &other {
            Other::File(path) => path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
            Other::Part(name) => name.clone(),
        };
        self.outside = Some(label.clone());
        self.notice = None;
        self.comparison = None;
        self.selected = None;
        self.viewport.set_model(None, false);
        self.shown = None;
        self.jobs.spawn_cancellable(ctx, move |progress| {
            let done = project.compare_outside(&part, pick, &other, progress);
            Done::ComparedOutside(part, label, done)
        });
    }

    /// Подпись сравнения вне истории: «v2 → файл.step».
    pub(super) fn outside_title(&self) -> Option<String> {
        let other = self.outside.as_ref()?;
        Some(format!("{} → {other}", pick_label(self.picks.1)))
    }

    /// Закрыть сравнение с файлом: вернуться к версиям детали.
    pub(super) fn close_outside(&mut self, ctx: &egui::Context) {
        self.outside = None;
        self.compare(ctx);
    }

    /// Деталь больше не называется `old`: всё, что помнит окно, — про `new`.
    pub(super) fn renamed(&mut self, ctx: &egui::Context, old: &str, new: String) {
        let t = texts();
        self.dialog = None;
        if self.expanded.remove(old) {
            self.expanded.insert(new.clone());
        }
        self.listed.remove(old);
        if self.framed.as_deref() == Some(old) {
            self.framed = Some(new.clone());
        }
        let text = format!("«{old}» {} «{new}»", t.renamed_to);
        self.reload_on(ctx, new, text);
    }

    /// Деталь удалена: её больше нет на экране.
    pub(super) fn part_gone(&mut self, ctx: &egui::Context, name: &str) {
        let t = texts();
        self.dialog = None;
        self.expanded.remove(name);
        self.listed.remove(name);
        if self.part.as_deref() == Some(name) {
            self.clear_part();
        }
        let text = format!("«{name}» {}", t.part_deleted);
        self.notice = Some(text.clone());
        self.sticky = Some(text);
        self.reload(ctx);
    }
}

fn confirm(part: &str) -> Confirm {
    Confirm {
        part: part.to_string(),
        ..Confirm::default()
    }
}

/// Версия, которую можно вернуть из меню: не последняя.
pub fn old_version(me: Pick, latest: Option<u32>) -> Option<u32> {
    match me {
        Pick::Version(n) if Some(n) != latest => Some(n),
        _ => None,
    }
}
