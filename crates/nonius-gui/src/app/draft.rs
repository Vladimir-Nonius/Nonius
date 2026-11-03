//! Черновик: работать можно сразу, без папки проекта. Новый проект живёт во
//! временной папке программы, пока его не сохранят: «Сохранить проект как…»
//! переносит его целиком, с историей, в выбранную папку. Закрыть
//! несохранённый черновик с деталями — только после вопроса.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui::{self, RichText};

use super::jobs::Done;
use super::project_tab::ProjectTab;
use super::App;
use crate::texts::texts;

/// Что закрывают, пока в черновике есть детали.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Closing {
    /// Вкладку на этом месте.
    Tab(usize),
    /// Окно целиком.
    Window,
}

impl App {
    /// Открыт ли во вкладке черновик.
    pub(super) fn is_draft(&self, tab: &ProjectTab) -> bool {
        match (&self.drafts, &tab.project) {
            (Some(d), Some(p)) => p.root().starts_with(d),
            _ => false,
        }
    }

    /// Черновик, который жалко потерять: в нём есть детали.
    fn draft_with_parts(&self, tab: &ProjectTab) -> bool {
        self.is_draft(tab) && !tab.parts.is_empty()
    }

    /// Новый черновик в новой вкладке (или в пустой).
    pub(super) fn new_draft(&mut self, ctx: &egui::Context) {
        let Some(drafts) = self.drafts.clone() else {
            return;
        };
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let dir = drafts.join(stamp.to_string()).join(texts().draft_name);
        self.create(ctx, dir, Vec::new());
    }

    /// «Сохранить проект как…»: перенести черновик в папку, которую выберут.
    pub(super) fn save_draft(&mut self, ctx: &egui::Context) {
        let t = texts();
        let Some(from) = self.tab.project.as_ref().map(|p| p.root().to_path_buf()) else {
            return;
        };
        if self.tab.jobs.busy() {
            self.tab.message = Some(t.wait_for_work.to_string());
            return;
        }
        let Some(to) = self.chooser.folder(&self.path_input) else {
            return;
        };
        // Проект закрывается: держать его открытым при переносе нельзя.
        self.tab = ProjectTab::default();
        let (mover, opener) = (self.mover.clone(), self.opener.clone());
        let drafts = self.drafts.clone();
        self.tab.jobs.spawn(ctx, move |_| match mover(&from, &to) {
            Ok(()) => {
                if let Some(stamp) = from.parent().filter(|p| Some(*p) != drafts.as_deref()) {
                    let _ = std::fs::remove_dir(stamp);
                }
                Done::Moved(opener(&to), None)
            }
            Err(e) => Done::Moved(opener(&from), Some(format!("{e:#}"))),
        });
    }

    /// Крестик на вкладке: черновик с деталями — сначала вопрос.
    pub(super) fn ask_close_tab(&mut self, ctx: &egui::Context, i: usize) {
        let tab = if i == self.at {
            &self.tab
        } else {
            &self.tabs[i]
        };
        if self.draft_with_parts(tab) {
            self.closing = Some(Closing::Tab(i));
            return;
        }
        let draft = self.is_draft(tab).then(|| root_of(tab));
        self.close_tab(ctx, i);
        if let Some(Some(root)) = draft {
            self.forget_draft(&root);
        }
    }

    /// Окно закрывают: несохранённые черновики с деталями — сначала вопрос.
    pub(super) fn watch_window_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.may_close {
            return;
        }
        let unsaved = (0..self.tabs.len()).any(|i| {
            let tab = if i == self.at {
                &self.tab
            } else {
                &self.tabs[i]
            };
            self.draft_with_parts(tab)
        });
        if unsaved {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.closing = Some(Closing::Window);
        }
    }

    /// Удалить папку черновика вместе с папкой-меткой времени.
    fn forget_draft(&self, root: &Path) {
        let Some(drafts) = &self.drafts else {
            return;
        };
        let stamp = root.parent().filter(|p| *p != drafts.as_path());
        if let Some(dir) = stamp.filter(|p| p.starts_with(drafts)) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// Закрыть вкладку и стереть черновик в ней.
    fn drop_draft(&mut self, ctx: &egui::Context, i: usize) {
        let tab = if i == self.at {
            &self.tab
        } else {
            &self.tabs[i]
        };
        let root = root_of(tab);
        self.close_tab(ctx, i);
        if let Some(root) = root {
            self.forget_draft(&root);
        }
    }
}

/// Папка проекта во вкладке.
fn root_of(tab: &ProjectTab) -> Option<PathBuf> {
    tab.project.as_ref().map(|p| p.root().to_path_buf())
}

/// Жёлтая полоса над открытым черновиком.
pub fn bar(app: &mut App, ctx: &egui::Context) {
    if !app.is_draft(&app.tab) {
        return;
    }
    let t = texts();
    let fill = ctx.style().visuals.warn_fg_color.gamma_multiply(0.15);
    egui::TopBottomPanel::top("draft-bar")
        .frame(egui::Frame::side_top_panel(&ctx.style()).fill(fill))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(t.draft_bar).color(ui.visuals().warn_fg_color));
                if ui.button(t.save_draft).clicked() {
                    app.save_draft(ctx);
                }
            });
        });
}

/// Окошко «Проект не сохранён».
pub fn ask(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let Some(closing) = app.closing else {
        return;
    };
    let mut answer = None;
    egui::Modal::new(egui::Id::new("draft-unsaved")).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.heading(t.draft_unsaved);
        ui.add_space(6.0);
        ui.label(t.draft_unsaved_text);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(t.dont_save).clicked() {
                answer = Some(Answer::Drop);
            }
            if ui.button(t.cancel).clicked() {
                answer = Some(Answer::Cancel);
            }
            if ui.button(t.save_draft).clicked() {
                answer = Some(Answer::Save);
            }
        });
    });
    let Some(answer) = answer else {
        return;
    };
    app.closing = None;
    match (answer, closing) {
        (Answer::Cancel, _) => {}
        (Answer::Drop, Closing::Tab(i)) => app.drop_draft(ctx, i),
        (Answer::Drop, Closing::Window) => {
            for i in (0..app.tabs.len()).rev() {
                let tab = if i == app.at { &app.tab } else { &app.tabs[i] };
                if app.is_draft(tab) {
                    app.drop_draft(ctx, i);
                }
            }
            app.may_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        (Answer::Save, Closing::Tab(i)) => {
            app.switch_to(ctx, i);
            app.save_draft(ctx);
        }
        (Answer::Save, Closing::Window) => {
            let first = (0..app.tabs.len()).find(|&i| {
                let tab = if i == app.at { &app.tab } else { &app.tabs[i] };
                app.draft_with_parts(tab)
            });
            if let Some(i) = first {
                app.switch_to(ctx, i);
                app.save_draft(ctx);
            }
        }
    }
}

/// Ответ в окошке.
#[derive(Clone, Copy)]
enum Answer {
    Drop,
    Cancel,
    Save,
}
