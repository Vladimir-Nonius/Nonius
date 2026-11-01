//! Что можно сделать с деталью: переименовать, удалить, вернуть к версии,
//! выгрузить, сравнить с файлом. Спрашивают окошки `part_dialogs.rs`, сама
//! работа идёт в фоне.

use eframe::egui;

use super::jobs::Done;
use super::part_dialogs::{Confirm, Dialog, Rename};
use super::{pick_label, App};
use crate::source::{Other, Pick};
use crate::texts::texts;

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
    pub(super) fn confirmed(&mut self, ctx: &egui::Context) {
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
    pub(super) fn recover(&mut self, ctx: &egui::Context, part: String) {
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
