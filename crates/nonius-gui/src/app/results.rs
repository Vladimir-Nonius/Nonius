//! Что делать с результатами фоновой работы. Устаревшие (человек уже выбрал
//! другую деталь или другие версии) тихо выбрасываются.

use eframe::egui;

use super::jobs::Done;
use super::part_dialogs::Dialog;
use super::App;
use crate::source::{default_picks, Pick, VersionEntry};
use crate::texts::texts;
use crate::view::Model;

impl App {
    /// Показать модель: камера остаётся, если это та же деталь.
    fn put_model(&mut self, name: &str, model: Option<Model>) {
        let keep = self.framed.as_deref() == Some(name);
        if model.is_some() {
            self.framed = Some(name.to_string());
        }
        self.viewport.set_model(model, keep);
    }

    pub(super) fn take_results(&mut self, ctx: &egui::Context) {
        let t = texts();
        if let (Some(scene), Some(name)) = (self.jobs.take_model(), self.part.clone()) {
            self.put_model(&name, Model::new(&scene));
        }
        for done in self.jobs.finished() {
            let current = |name: &str| self.part.as_deref() == Some(name);
            match done {
                Done::Opened(Ok(p)) => {
                    self.set_project(ctx, p);
                    // Остальное в этой пачке — от прошлого проекта.
                    break;
                }
                Done::Opened(Err(e)) => {
                    self.after_open.clear();
                    self.message = Some(format!("{}: {e:#}", t.project_open_failed))
                }
                Done::Reread(Ok(parts)) => self.got_parts(ctx, parts),
                Done::Reread(Err(_)) => {}
                Done::Parts(Ok(parts)) => {
                    self.set_parts(ctx, parts);
                    let models = std::mem::take(&mut self.after_open);
                    self.start_adding(models, None);
                    let want = self.want.as_ref().map(|w| w.0.clone());
                    let only = (self.parts.len() == 1).then(|| self.parts[0].name.clone());
                    if let Some(name) = want.or(only) {
                        if self.parts.iter().any(|p| p.name == name) {
                            self.select_part(ctx, &name);
                        }
                    }
                }
                Done::Versions(name, Ok(v)) if current(&name) => self.got_versions(ctx, &name, v),
                Done::Refreshed(parts, name, versions) => {
                    self.set_parts(ctx, parts);
                    match versions {
                        Ok(v) if self.part.as_deref() == Some(&name) => {
                            // «Сохранена версия …» остаётся видна и после
                            // нового сравнения.
                            let notice = self.notice.take();
                            self.got_versions(ctx, &name, v);
                            self.notice = notice;
                        }
                        Ok(_) => {}
                        Err(e) => self.message = Some(format!("{}: {e:#}", t.error)),
                    }
                }
                Done::Compared(name, picks, Ok(c)) if current(&name) && self.picks == picks => {
                    let model = c.scene.as_ref().and_then(Model::new);
                    self.put_model(&name, model);
                    self.shown = Some((name, picks));
                    self.comparison = Some(c);
                    self.selected = None;
                }
                Done::Masses(name, picks, Ok(m)) if current(&name) && self.picks == picks => {
                    self.masses = Some((picks, m));
                }
                Done::VersionFiles(name, picks, Ok(f)) if current(&name) && self.picks == picks => {
                    self.got_files(ctx, f);
                }
                Done::VersionFiles(name, picks, Err(e))
                    if current(&name) && self.picks == picks =>
                {
                    self.message = Some(format!("{}: {e:#}", t.error))
                }
                Done::FileDiff(name, picks, path, lines)
                    if current(&name)
                        && self.picks == picks
                        && self.file.as_deref() == Some(&path) =>
                {
                    match lines {
                        Ok(l) => self.file_text = Some(l),
                        Err(e) => self.message = Some(format!("{}: {e:#}", t.error)),
                    }
                }
                Done::FileOpened(_, true, Ok(path)) => {
                    self.notice = Some(format!("{}: {}", t.folder_shown, path.display()));
                }
                Done::FileOpened(pick, false, Ok(path)) => {
                    let copy = if matches!(pick, Pick::Version(_)) {
                        t.opened_copy
                    } else {
                        ""
                    };
                    self.notice = Some(format!("{}: {}{copy}", t.file_opened, path.display()));
                }
                Done::FileOpened(_, _, Err(e)) => {
                    self.message = Some(format!("{}: {e:#}", t.error))
                }
                Done::Programs(kind, list) => {
                    self.programs.insert(kind, Some(list));
                }
                Done::Files(name, files) => {
                    if let Some(d) = self.save.as_mut().filter(|d| d.part == name) {
                        match files {
                            Ok(f) => d.files = Some(f),
                            Err(e) => d.error = Some(format!("{e:#}")),
                        }
                    }
                }
                Done::Saved(name, Ok(n)) => {
                    self.save = None;
                    self.notice = Some(format!("{} «{name}» v{n}", t.version_saved));
                    self.refresh(ctx);
                }
                Done::Saved(name, Err(e)) => match self.save.as_mut().filter(|d| d.part == name) {
                    Some(d) => {
                        d.saving = false;
                        d.error = Some(format!("{e:#}"));
                    }
                    None => self.message = Some(format!("{}: {e:#}", t.error)),
                },
                Done::MaterialSet(name, Ok(m)) => {
                    self.material_busy = false;
                    let material = m.material.as_deref().unwrap_or(t.no_material);
                    self.notice = Some(format!(
                        "«{name}»: {} — {material}. {}",
                        t.material, t.material_set
                    ));
                    self.custom.open = false;
                    // Материал меняет паспорт: деталь стала изменённой,
                    // сравнение и масса — другие.
                    if current(&name) {
                        self.want = Some((name, None));
                        self.refresh(ctx);
                    }
                }
                Done::MaterialSet(_, Err(e)) => {
                    self.material_busy = false;
                    self.message = Some(format!("{}: {e:#}", t.error));
                }
                Done::Masses(name, picks, Err(e)) if current(&name) && self.picks == picks => {
                    self.message = Some(format!("{}: {e:#}", t.error))
                }
                Done::Parts(Err(e)) | Done::Versions(_, Err(e)) | Done::Compared(_, _, Err(e)) => {
                    self.message = Some(format!("{}: {e:#}", t.error))
                }
                Done::PartCreated(name, Ok(())) => {
                    self.new_part = None;
                    let text = format!("{} «{name}»", t.part_created);
                    self.reload_on(ctx, name, text);
                }
                Done::PartCreated(_, Err(e)) => match self.new_part.as_mut() {
                    Some(d) => d.failed(format!("{e:#}")),
                    None => self.message = Some(format!("{}: {e:#}", t.error)),
                },
                Done::Added(Ok(added)) => self.added(ctx, added),
                Done::Added(Err(e)) => match self.add.as_mut() {
                    Some(d) => d.failed(format!("{e:#}")),
                    None => self.message = Some(format!("{}: {e:#}", t.error)),
                },
                Done::Listed(name, Ok(v)) => {
                    self.listed.insert(name, v);
                }
                Done::Listed(_, Err(e)) => self.message = Some(format!("{}: {e:#}", t.error)),
                Done::PartRenamed(old, new, Ok(())) => self.renamed(ctx, &old, new),
                Done::PartDeleted(name, Ok(())) => self.part_gone(ctx, &name),
                Done::PartRecovered(name, Ok(())) => {
                    self.dialog = None;
                    let text = format!("«{name}» {}", t.part_recovered);
                    self.reload_on(ctx, name, text);
                }
                Done::Restored(name, version, Ok(n)) => {
                    self.dialog = None;
                    self.notice = Some(format!("«{name}» {} v{version}: v{n}", t.restored_to));
                    self.refresh(ctx);
                }
                Done::FilesAdded(name, result) => match result {
                    Ok(paths) => {
                        let text = format!(
                            "«{name}»: {} {}. {}",
                            t.files_added,
                            paths.join(", "),
                            t.save_to_keep
                        );
                        self.reload_on(ctx, name, text);
                    }
                    Err(e) => self.message = Some(format!("{}: {e:#}", t.error)),
                },
                Done::FileRemoved(name, path, Ok(())) => {
                    self.dialog = None;
                    let text = format!("{path} {} «{name}». {}", t.file_removed, t.save_to_keep);
                    self.reload_on(ctx, name, text);
                }
                Done::PartRenamed(_, _, Err(e))
                | Done::FileRemoved(_, _, Err(e))
                | Done::PartDeleted(_, Err(e))
                | Done::PartRecovered(_, Err(e))
                | Done::Restored(_, _, Err(e)) => match self.dialog.as_mut() {
                    Some(d) => d.failed(format!("{e:#}")),
                    None => self.message = Some(format!("{}: {e:#}", t.error)),
                },
                Done::DeletedList(result) => match (self.dialog.as_mut(), result) {
                    (Some(Dialog::Deleted(list, _)), Ok(found)) => *list = Some(found),
                    (Some(d), Err(e)) => d.failed(format!("{e:#}")),
                    _ => {}
                },
                Done::Exported(name, version, dest, result) => match result {
                    Ok(n) => {
                        let place = dest.display();
                        self.notice =
                            Some(format!("«{name}» v{version}: {} {n}: {place}", t.exported));
                    }
                    Err(e) => self.message = Some(format!("{}: {e:#}", t.error)),
                },
                Done::ComparedOutside(name, label, result)
                    if current(&name) && self.outside.as_ref() == Some(&label) =>
                {
                    match result {
                        Ok(c) => {
                            let model = c.scene.as_ref().and_then(Model::new);
                            self.put_model(&name, model);
                            self.comparison = Some(c);
                            self.selected = None;
                        }
                        Err(e) => {
                            self.message = Some(format!("{}: {e:#}", t.error));
                            self.close_outside(ctx);
                        }
                    }
                }
                Done::Crashed => self.message = Some(t.crashed.to_string()),
                // Устарело: выбрали другое.
                Done::ComparedOutside(..) => {}
                Done::Versions(..)
                | Done::Compared(..)
                | Done::Masses(..)
                | Done::VersionFiles(..)
                | Done::FileDiff(..) => {}
            }
        }
    }

    /// Пришли версии детали: выбрать пару и сравнить.
    fn got_versions(&mut self, ctx: &egui::Context, name: &str, v: Vec<VersionEntry>) {
        let entry = self.parts.iter().find(|p| p.name == name);
        let defaults = entry.map(|e| default_picks(e, &v));
        let wanted = match self.want.take() {
            Some((n, picks)) if n == name => picks,
            _ => None,
        };
        self.versions = v;
        let picks = wanted.or(defaults).unwrap_or(self.picks);
        self.show_picks(ctx, picks);
        if let Some(text) = self.sticky.take() {
            self.notice = Some(text);
        }
    }
}
