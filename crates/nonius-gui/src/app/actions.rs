//! Что окно делает по просьбе человека: открыть проект, выбрать деталь и
//! версии, сравнить, сохранить версию, назначить материал. Долгое — в фоне.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

use super::jobs::Done;
use super::App;
use crate::source::{MassEntry, PartEntry, Pick, ProjectSource};
use crate::texts::texts;

impl App {
    /// Открыть проект по папке (в фоне).
    pub(super) fn open(&mut self, ctx: &egui::Context, path: PathBuf) {
        let opener = self.opener.clone();
        self.message = None;
        self.want = None;
        self.jobs.spawn(ctx, move |_| Done::Opened(opener(&path)));
    }

    /// Создать проект в папке (в фоне), открыть его и добавить модели.
    pub(super) fn create(&mut self, ctx: &egui::Context, path: PathBuf, models: Vec<PathBuf>) {
        let creator = self.creator.clone();
        self.message = None;
        self.want = None;
        self.after_open = models;
        self.jobs.spawn(ctx, move |_| Done::Opened(creator(&path)));
    }

    pub(super) fn set_project(&mut self, ctx: &egui::Context, project: Box<dyn ProjectSource>) {
        self.settings.last_project = Some(project.root().to_path_buf());
        self.settings.save();
        let project: Arc<dyn ProjectSource> = Arc::from(project);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{} — {}",
            project.title(),
            texts().app_title
        )));
        self.materials = project.materials();
        self.project = Some(project.clone());
        // Всё от прошлого проекта: и работы, и то, что на экране.
        self.jobs.forget_all();
        self.parts.clear();
        self.expanded.clear();
        self.listed.clear();
        self.dialog = None;
        self.select(None);
        self.viewport.set_model(None, false);
        self.shown = None;
        self.framed = None;
        self.picks = (Pick::Current, Pick::Current);
        self.material_busy = false;
        self.new_part = None;
        self.add = None;
        self.new_here = None;
        self.sticky = None;
        self.search.clear();
        self.jobs.spawn(ctx, move |_| Done::Parts(project.parts()));
    }

    /// Выбрать деталь: прочитать её версии, потом сравнить.
    pub(super) fn select_part(&mut self, ctx: &egui::Context, name: &str) {
        let Some(project) = self.project.clone() else {
            return;
        };
        self.select(Some(name.to_string()));
        self.expanded.insert(name.to_string());
        let name = name.to_string();
        self.jobs.spawn(ctx, move |_| {
            let v = project.versions(&name);
            Done::Versions(name, v)
        });
    }

    /// Раскрыть деталь в дереве, не выбирая её.
    pub(super) fn list_versions(&mut self, ctx: &egui::Context, name: &str) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let name = name.to_string();
        self.jobs.spawn(ctx, move |_| {
            let v = project.versions(&name);
            Done::Listed(name, v)
        });
    }

    /// Новый список деталей: раскрытые, что изменились, перечитать.
    pub(super) fn set_parts(&mut self, ctx: &egui::Context, parts: Vec<PartEntry>) {
        let old = std::mem::replace(&mut self.parts, parts);
        let names = self.parts.iter().map(|p| p.name.clone());
        let names: std::collections::BTreeSet<String> = names.collect();
        self.expanded.retain(|n| names.contains(n));
        self.listed.retain(|n, _| names.contains(n));
        let stale: Vec<String> = self
            .expanded
            .iter()
            .filter(|n| Some(*n) != self.part.as_ref())
            .filter(|n| {
                let entry = |list: &[PartEntry]| list.iter().find(|p| p.name == **n).cloned();
                entry(&old) != entry(&self.parts)
            })
            .cloned()
            .collect();
        for name in stale {
            self.list_versions(ctx, &name);
        }
    }

    /// Убрать выбранную деталь с экрана.
    pub(super) fn clear_part(&mut self) {
        self.select(None);
        self.viewport.set_model(None, false);
        self.shown = None;
        self.framed = None;
    }

    /// Перечитать список деталей.
    pub(super) fn reload(&mut self, ctx: &egui::Context) {
        if let Some(project) = self.project.clone() {
            self.jobs.spawn(ctx, move |_| Done::Parts(project.parts()));
        }
    }

    fn select(&mut self, part: Option<String>) {
        self.jobs.cancel();
        self.part = part;
        self.versions.clear();
        self.comparison = None;
        self.masses = None;
        self.selected = None;
        self.message = None;
        self.notice = None;
        self.save = None;
        self.outside = None;
        self.files = None;
        self.file = None;
        self.file_text = None;
    }

    /// Заметить возврат в окно: модель могли поправить в SolidWorks.
    pub(super) fn notice_focus(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.focused);
        self.reread_wanted |= focused && !self.was_focused;
        self.was_focused = focused;
        let free =
            !self.jobs.busy() && self.save.is_none() && self.add.is_none() && self.dialog.is_none();
        if let (true, true, Some(project)) = (self.reread_wanted, free, self.project.clone()) {
            self.reread_wanted = false;
            self.jobs.spawn(ctx, move |_| Done::Reread(project.parts()));
        }
    }

    /// Пришёл свежий список деталей. Если выбранная изменилась — показать
    /// заново; если её больше нет — снять выбор.
    pub(super) fn got_parts(&mut self, ctx: &egui::Context, parts: Vec<crate::source::PartEntry>) {
        let find = |list: &[crate::source::PartEntry], name: &str| {
            list.iter().find(|p| p.name == name).cloned()
        };
        let chosen = self.part.clone();
        let before = chosen.as_deref().and_then(|n| find(&self.parts, n));
        let after = chosen.as_deref().and_then(|n| find(&parts, n));
        self.set_parts(ctx, parts);
        if chosen.is_some() && after.is_none() {
            self.clear_part();
        } else if before != after {
            self.shown = None;
            self.refresh(ctx);
        }
    }

    /// Перечитать детали и версии выбранной детали после сохранения или
    /// смены материала. Показ остаётся, пока не придёт новое сравнение.
    pub(super) fn refresh(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(name)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        self.jobs.spawn(ctx, move |_| match project.parts() {
            Ok(parts) => {
                let versions = project.versions(&name);
                Done::Refreshed(parts, name, versions)
            }
            Err(e) => Done::Parts(Err(e)),
        });
    }

    /// Показать пару версий: сравнить её и прочитать массы.
    pub(super) fn show_picks(&mut self, ctx: &egui::Context, picks: (Pick, Pick)) {
        self.picks = picks;
        self.compare(ctx);
    }

    /// Сравнить выбранную пару версий. Прошлое сравнение, если ещё идёт,
    /// отменяется: его результат уже не нужен.
    pub(super) fn compare(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(part)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        let picks = self.picks;
        self.notice = None;
        self.outside = None;
        // Старые цвета и числа — не про эту пару версий: пока считаем, их нет.
        self.comparison = None;
        self.selected = None;
        if self.shown.as_ref() != Some(&(part.clone(), picks)) {
            self.viewport.set_model(None, false);
        }
        let (p, name) = (project, part);
        self.jobs.spawn_cancellable(ctx, move |progress| {
            let result = p.compare(&name, picks.0, picks.1, progress);
            Done::Compared(name, picks, result)
        });
        self.load_masses(ctx);
        self.load_files(ctx);
    }

    /// Прочитать материал и массу для выбранной пары версий и сейчас.
    pub(super) fn load_masses(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(part)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        let picks = self.picks;
        self.jobs.spawn(ctx, move |_| {
            // Одну и ту же версию не читать дважды: «Стало» часто — «сейчас».
            let mut read: Vec<(Pick, MassEntry)> = Vec::new();
            let mut mass = |pick: Pick| -> anyhow::Result<MassEntry> {
                if let Some((_, m)) = read.iter().find(|(p, _)| *p == pick) {
                    return Ok(m.clone());
                }
                let m = project.mass(&part, pick)?;
                read.push((pick, m.clone()));
                Ok(m)
            };
            let result = (|| Ok([mass(picks.0)?, mass(picks.1)?, mass(Pick::Current)?]))();
            Done::Masses(part, picks, result)
        });
    }

    /// «Отменить» в строке состояния: бросить сравнение и вернуться к паре
    /// версий, которая была показана последней. Она в кэше, это быстро.
    pub(super) fn cancel_compare(&mut self, ctx: &egui::Context) {
        if !self.jobs.cancel() {
            return;
        }
        let last = self
            .shown
            .clone()
            .filter(|s| self.part.as_ref() == Some(&s.0));
        if let Some((_, picks)) = last.filter(|s| s.1 != self.picks) {
            self.show_picks(ctx, picks);
        }
        self.notice = Some(texts().compare_cancelled.to_string());
    }

    /// Открыть окошко «Сохранить версию» и прочитать изменённые файлы.
    pub(super) fn start_save(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(part)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        let latest = self.versions.first().map(|v| v.number);
        let next = latest.map_or(1, |n| n + 1);
        self.save = Some(super::save::SaveDialog::new(&part, next));
        // Показать в 3D именно то, что попадёт в новую версию.
        if let Some(n) = latest {
            let picks = (Pick::Version(n), Pick::Current);
            if self.picks != picks {
                self.show_picks(ctx, picks);
            }
        }
        self.jobs.spawn(ctx, move |_| {
            let files = project.changed_files(&part);
            Done::Files(part, files)
        });
    }

    /// Сохранить версию с описанием из окошка.
    pub(super) fn save_version(&mut self, ctx: &egui::Context) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let Some(dialog) = self.save.as_mut() else {
            return;
        };
        dialog.saving = true;
        dialog.error = None;
        let (part, message) = (dialog.part.clone(), dialog.message.clone());
        // После сохранения всё перечитается; идущее сравнение не нужно.
        self.jobs.cancel();
        self.jobs.spawn(ctx, move |_| {
            let saved = project.save_version(&part, &message);
            Done::Saved(part, saved)
        });
    }

    /// Назначить материал детали сейчас. `None` — убрать.
    pub(super) fn set_material(
        &mut self,
        ctx: &egui::Context,
        material: Option<(String, Option<f64>)>,
    ) {
        let (Some(project), Some(part)) = (self.project.clone(), self.part.clone()) else {
            return;
        };
        // Следующий выбор — только когда этот записан: иначе неизвестно,
        // какой материал окажется в паспорте.
        self.material_busy = true;
        self.jobs.cancel();
        self.jobs.spawn(ctx, move |_| {
            let set = match &material {
                Some((name, density)) => project.set_material(&part, Some(name), *density),
                None => project.set_material(&part, None, None),
            };
            Done::MaterialSet(part, set)
        });
    }

    /// Создать деталь из окошка «Новая деталь».
    pub(super) fn create_part(&mut self, ctx: &egui::Context, name: String, assembly: bool) {
        let Some(project) = self.project.clone() else {
            return;
        };
        self.jobs.spawn(ctx, move |_| {
            let made = project.new_part(&name, assembly);
            Done::PartCreated(name, made)
        });
    }

    /// Деталь появилась или изменилась: перечитать список и выбрать её.
    /// `text` остаётся в строке состояния и после нового сравнения.
    pub(super) fn reload_on(&mut self, ctx: &egui::Context, part: String, text: String) {
        let Some(project) = self.project.clone() else {
            return;
        };
        self.want = Some((part, None));
        self.sticky = Some(text);
        // Новая деталь не должна спрятаться за старым поиском.
        self.search.clear();
        self.jobs.spawn(ctx, move |_| Done::Parts(project.parts()));
    }

    /// Человек поменял переключатель — запомнить.
    pub(super) fn remember_look(&mut self) {
        let s = &mut self.settings;
        if (s.xray, s.edges) != (self.look.xray, self.look.edges) {
            s.xray = self.look.xray;
            s.edges = self.look.edges;
            s.save();
        }
    }
}
