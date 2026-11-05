//! Что окно делает по просьбе человека: открыть проект, выбрать деталь и
//! версии, сравнить, сохранить версию, назначить материал. Долгое — в фоне.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

use super::jobs::Done;
use super::node::Node;
use super::App;
use crate::source::{MassEntry, Pick, ProjectSource};
use crate::texts::texts;

impl App {
    /// Открыть проект по папке (в фоне).
    pub(super) fn open(&mut self, ctx: &egui::Context, path: PathBuf) {
        let opener = self.opener.clone();
        self.tab.message = None;
        self.tab.want = None;
        self.tab
            .jobs
            .spawn(ctx, move |_| Done::Opened(opener(&path)));
    }

    /// Создать проект в папке (в фоне), открыть его и добавить модели.
    pub(super) fn create(&mut self, ctx: &egui::Context, path: PathBuf, models: Vec<PathBuf>) {
        let creator = self.creator.clone();
        self.tab.message = None;
        self.tab.want = None;
        self.tab.after_open = models;
        self.tab
            .jobs
            .spawn(ctx, move |_| Done::Opened(creator(&path)));
    }

    pub(super) fn set_project(&mut self, ctx: &egui::Context, project: Box<dyn ProjectSource>) {
        let project: Arc<dyn ProjectSource> = Arc::from(project);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{} — {}",
            project.title(),
            texts().app_title
        )));
        self.tab.materials = project.materials();
        self.tab.ask_upgrade = project.old_layout();
        self.tab.upgrading = false;
        self.tab.project = Some(project.clone());
        // Всё от прошлого проекта: и работы, и то, что на экране.
        self.tab.jobs.forget_all();
        self.tab.batch = None;
        self.tab.parts.clear();
        self.tab.expanded.clear();
        self.tab.folders.clear();
        self.tab.loose.clear();
        self.tab.folded.clear();
        self.tab.place.clear();
        self.tab.selection.clear();
        self.tab.new_folder = None;
        self.tab.listed.clear();
        self.tab.dialog = None;
        self.select(None);
        self.viewport.set_model(None, false);
        self.tab.shown = None;
        self.tab.framed = None;
        self.tab.picks = (Pick::Current, Pick::Current);
        self.tab.material_busy = false;
        self.tab.new_part = None;
        self.tab.add = None;
        self.new_here = None;
        self.tab.sticky = None;
        self.tab.search.clear();
        self.tab
            .jobs
            .spawn(ctx, move |_| Done::Parts(project.listing()));
        self.remember_tabs();
    }

    /// Выбрать деталь: прочитать её версии, потом сравнить.
    pub(super) fn select_part(&mut self, ctx: &egui::Context, name: &str) {
        let Some(project) = self.tab.project.clone() else {
            return;
        };
        self.select(Some(name.to_string()));
        self.tab.expanded.insert(name.to_string());
        let name = name.to_string();
        self.tab.jobs.spawn(ctx, move |_| {
            let v = project.versions(&name);
            Done::Versions(name, v)
        });
    }

    /// Убрать выбранную деталь с экрана.
    pub(super) fn clear_part(&mut self) {
        self.select(None);
        self.viewport.set_model(None, false);
        self.tab.shown = None;
        self.tab.framed = None;
    }

    /// Перечитать список деталей.
    pub(super) fn reload(&mut self, ctx: &egui::Context) {
        if let Some(project) = self.tab.project.clone() {
            self.tab
                .jobs
                .spawn(ctx, move |_| Done::Parts(project.listing()));
        }
    }

    fn select(&mut self, part: Option<String>) {
        self.tab.jobs.cancel();
        // Открытая деталь выделена одна: Delete и меню — про неё.
        if let Some(name) = &part {
            self.tab.selection.only(Node::Part(name.clone()));
        }
        self.tab.keys_to_files = false;
        // Прежняя деталь может остаться раскрытой: её версии не пропадают.
        let old = std::mem::replace(&mut self.tab.part, part);
        let versions = std::mem::take(&mut self.tab.versions);
        if let Some(old) = old.filter(|_| !versions.is_empty()) {
            self.tab.listed.insert(old, versions);
        }
        self.tab.comparison = None;
        self.tab.masses = None;
        self.tab.selected = None;
        self.tab.message = None;
        self.tab.notice = None;
        self.tab.save = None;
        self.tab.outside = None;
        self.tab.files = None;
        self.tab.file = None;
        self.tab.file_text = None;
    }

    /// Заметить возврат в окно: модель могли поправить в SolidWorks.
    pub(super) fn notice_focus(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.focused);
        self.tab.reread_wanted |= focused && !self.was_focused;
        self.was_focused = focused;
        let free = !self.tab.jobs.busy()
            && self.tab.save.is_none()
            && self.tab.add.is_none()
            && self.tab.dialog.is_none();
        if let (true, true, Some(project)) =
            (self.tab.reread_wanted, free, self.tab.project.clone())
        {
            self.tab.reread_wanted = false;
            self.tab
                .jobs
                .spawn(ctx, move |_| Done::Reread(project.listing()));
        }
    }

    /// Пришёл свежий список деталей. Если выбранная изменилась — показать
    /// заново; если её больше нет — снять выбор.
    pub(super) fn got_parts(&mut self, ctx: &egui::Context, listing: crate::source::Listing) {
        // Перенос в другую папку показ не меняет: папку не сравниваем.
        let find = |list: &[crate::source::PartEntry], name: &str| {
            let p = list.iter().find(|p| p.name == name)?;
            Some(crate::source::PartEntry {
                folder: String::new(),
                ..p.clone()
            })
        };
        let chosen = self.tab.part.clone();
        let before = chosen.as_deref().and_then(|n| find(&self.tab.parts, n));
        let after = chosen.as_deref().and_then(|n| find(&listing.parts, n));
        self.set_parts(ctx, listing);
        if chosen.is_some() && after.is_none() {
            self.clear_part();
        } else if before != after {
            self.tab.shown = None;
            self.refresh(ctx);
        }
    }

    /// Перечитать детали и версии выбранной детали после сохранения или
    /// смены материала. Показ остаётся, пока не придёт новое сравнение.
    pub(super) fn refresh(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(name)) = (self.tab.project.clone(), self.tab.part.clone()) else {
            return;
        };
        self.tab.jobs.spawn(ctx, move |_| match project.listing() {
            Ok(listing) => {
                let versions = project.versions(&name);
                Done::Refreshed(listing, name, versions)
            }
            Err(e) => Done::Parts(Err(e)),
        });
    }

    /// Показать пару версий: сравнить её и прочитать массы.
    pub(super) fn show_picks(&mut self, ctx: &egui::Context, picks: (Pick, Pick)) {
        self.tab.picks = picks;
        self.compare(ctx);
    }

    /// Сравнить выбранную пару версий. Прошлое сравнение, если ещё идёт,
    /// отменяется: его результат уже не нужен.
    pub(super) fn compare(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(part)) = (self.tab.project.clone(), self.tab.part.clone()) else {
            return;
        };
        let picks = self.tab.picks;
        self.tab.notice = None;
        self.tab.outside = None;
        // Старые цвета и числа — не про эту пару версий: пока считаем, их нет.
        self.tab.comparison = None;
        self.tab.selected = None;
        if self.tab.shown.as_ref() != Some(&(part.clone(), picks)) {
            self.viewport.set_model(None, false);
        }
        let (p, name) = (project, part);
        self.tab.jobs.spawn_cancellable(ctx, move |progress| {
            let result = p.compare(&name, picks.0, picks.1, progress);
            Done::Compared(name, picks, result)
        });
        self.load_masses(ctx);
        self.load_files(ctx);
    }

    /// Прочитать материал и массу для выбранной пары версий и сейчас.
    pub(super) fn load_masses(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(part)) = (self.tab.project.clone(), self.tab.part.clone()) else {
            return;
        };
        let picks = self.tab.picks;
        self.tab.jobs.spawn(ctx, move |_| {
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
        if !self.tab.jobs.cancel() {
            return;
        }
        let last = self
            .tab
            .shown
            .clone()
            .filter(|s| self.tab.part.as_ref() == Some(&s.0));
        if let Some((_, picks)) = last.filter(|s| s.1 != self.tab.picks) {
            self.show_picks(ctx, picks);
        }
        self.tab.notice = Some(texts().compare_cancelled.to_string());
    }

    /// Открыть окошко «Сохранить версию» и прочитать изменённые файлы.
    pub(super) fn start_save(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(part)) = (self.tab.project.clone(), self.tab.part.clone()) else {
            return;
        };
        let latest = self.tab.versions.first().map(|v| v.number);
        let next = latest.map_or(1, |n| n + 1);
        self.tab.save = Some(super::save::SaveDialog::new(&part, next));
        // Показать в 3D именно то, что попадёт в новую версию.
        if let Some(n) = latest {
            let picks = (Pick::Version(n), Pick::Current);
            if self.tab.picks != picks {
                self.show_picks(ctx, picks);
            }
        }
        self.tab.jobs.spawn(ctx, move |_| {
            let files = project.changed_files(&part);
            Done::Files(part, files)
        });
    }

    /// Сохранить версию с описанием из окошка.
    pub(super) fn save_version(&mut self, ctx: &egui::Context) {
        let Some(project) = self.tab.project.clone() else {
            return;
        };
        let Some(dialog) = self.tab.save.as_mut() else {
            return;
        };
        dialog.saving = true;
        dialog.error = None;
        let (part, message) = (dialog.part.clone(), dialog.message.clone());
        // После сохранения всё перечитается; идущее сравнение не нужно.
        self.tab.jobs.cancel();
        self.tab.jobs.spawn(ctx, move |_| {
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
        let (Some(project), Some(part)) = (self.tab.project.clone(), self.tab.part.clone()) else {
            return;
        };
        // Следующий выбор — только когда этот записан: иначе неизвестно,
        // какой материал окажется в паспорте.
        self.tab.material_busy = true;
        self.tab.jobs.cancel();
        self.tab.jobs.spawn(ctx, move |_| {
            let set = match &material {
                Some((name, density)) => project.set_material(&part, Some(name), *density),
                None => project.set_material(&part, None, None),
            };
            Done::MaterialSet(part, set)
        });
    }

    /// Создать деталь из окошка «Новая деталь».
    pub(super) fn create_part(
        &mut self,
        ctx: &egui::Context,
        name: String,
        assembly: bool,
        folder: String,
    ) {
        let Some(project) = self.tab.project.clone() else {
            return;
        };
        self.tab.jobs.spawn(ctx, move |_| {
            let made = project.new_part(&name, assembly, &folder);
            Done::PartCreated(name, made)
        });
    }

    /// Деталь появилась или изменилась: перечитать список и выбрать её.
    /// `text` остаётся в строке состояния и после нового сравнения.
    pub(super) fn reload_on(&mut self, ctx: &egui::Context, part: String, text: String) {
        let Some(project) = self.tab.project.clone() else {
            return;
        };
        self.tab.want = Some((part, None));
        self.tab.sticky = Some(text);
        // Новая деталь не должна спрятаться за старым поиском.
        self.tab.search.clear();
        self.tab
            .jobs
            .spawn(ctx, move |_| Done::Parts(project.listing()));
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
