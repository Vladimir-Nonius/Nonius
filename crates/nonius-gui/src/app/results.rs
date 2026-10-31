//! Что делать с результатами фоновой работы. Устаревшие (человек уже выбрал
//! другую деталь или другие версии) тихо выбрасываются.

use eframe::egui;

use super::jobs::Done;
use super::App;
use crate::source::{default_picks, VersionEntry};
use crate::texts::texts;
use crate::view::Model;

impl App {
    pub(super) fn take_results(&mut self, ctx: &egui::Context) {
        let t = texts();
        for done in self.jobs.finished() {
            let current = |name: &str| self.part.as_deref() == Some(name);
            match done {
                Done::Opened(Ok(p)) => self.set_project(ctx, p),
                Done::Opened(Err(e)) => {
                    self.message = Some(format!("{}: {e:#}", t.project_open_failed))
                }
                Done::Parts(Ok(parts)) => {
                    self.parts = parts;
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
                    self.parts = parts;
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
                    let same_part = self.shown.as_ref().is_some_and(|s| s.0 == name);
                    let model = c.scene.as_ref().and_then(Model::new);
                    self.viewport.set_model(model, same_part);
                    self.shown = Some((name, picks));
                    self.comparison = Some(c);
                    self.selected = None;
                }
                Done::Masses(name, picks, Ok(m)) if current(&name) && self.picks == picks => {
                    self.masses = Some((picks, m));
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
                Done::Crashed => self.message = Some(t.crashed.to_string()),
                // Устарело: выбрали другое.
                Done::Versions(..) | Done::Compared(..) | Done::Masses(..) => {}
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
        if self.versions.is_empty() {
            self.picks = picks;
            self.viewport.set_model(None, false);
            self.shown = None;
            self.comparison = None;
            self.load_masses(ctx);
        } else {
            self.show_picks(ctx, picks);
        }
    }
}
