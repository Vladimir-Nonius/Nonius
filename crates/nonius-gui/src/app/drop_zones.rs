//! Куда бросили файл из Проводника. Система отдаёт окну только сами файлы,
//! без места: строки дерева и блоки «Структуры» каждый кадр отмечают, где
//! они на экране (`shown.rs`), а место броска — курсор, пока файл несли
//! (`drag_hint.rs` показывает это у курсора). Здесь же — открыть обычный
//! файл папки.

use std::path::PathBuf;

use eframe::egui;

use super::drag_hint::Carry;
use super::file_menu::is_model;
use super::jobs::Done;
use super::node::Node;
use super::shown::{self, Shown};
use super::App;
use crate::source::{OpenWay, Pick};
use crate::texts::texts;

impl App {
    /// Где сейчас курсор в окне: у системы, а без неё — у egui.
    pub(super) fn pointer_in_window(&self, ctx: &egui::Context) -> Option<egui::Pos2> {
        let Some((x, y)) = self.chooser.cursor() else {
            return ctx.input(|i| i.pointer.latest_pos());
        };
        let ppp = ctx.pixels_per_point();
        let inner = ctx.input(|i| i.viewport().inner_rect)?;
        Some(egui::pos2(x / ppp, y / ppp) - inner.min.to_vec2())
    }

    /// Куда ляжет файл из Проводника, брошенный под курсором.
    fn landing_under(&self, ctx: &egui::Context) -> Option<Shown> {
        self.landing_at(self.pointer_in_window(ctx)?, Carry::Copy)
    }

    /// Куда ляжет брошенное в эту точку и что подсветить. Брошенное на файл
    /// ложится в его папку; деталь, брошенная на деталь, — тоже в папку,
    /// а файл из Проводника — в саму деталь. Подсвечивается всё место, куда
    /// ляжет: папка целиком, а не строка, над которой курсор.
    pub(super) fn landing_at(&self, pos: egui::Pos2, carry: Carry) -> Option<Shown> {
        let found = shown::under(&self.tab.on_screen, pos)?;
        let node = match (&found.node, carry) {
            (Node::Part(p), Carry::Move) => Node::Folder(self.folder_of(p)),
            (node, _) => node.landing(),
        };
        let rect = shown::whole(&self.tab.on_screen, &node, pos).unwrap_or(found.rect);
        Some(Shown::new(rect, node))
    }

    /// Папка, где лежит деталь.
    pub(super) fn folder_of(&self, part: &str) -> String {
        let p = self.tab.parts.iter().find(|p| p.name == part);
        p.map(|p| p.folder.clone()).unwrap_or_default()
    }

    /// Файлы бросили: на папку — модели новыми деталями в неё, остальное
    /// копией туда же; на деталь — модель новой версией, остальное в `docs/`. Мимо дерева — `None`, решает обычный
    /// путь (модели добавить, папку проекта открыть).
    pub(super) fn drop_on_zone(&mut self, ctx: &egui::Context, paths: &[PathBuf]) -> bool {
        if self.tab.project.is_none() || paths.iter().any(|p| p.is_dir()) {
            return false;
        }
        // Цель запомнена, пока файл несли; без неё — где курсор сейчас.
        let aim = self.tab.drop_aim.take();
        let Some(node) = aim.or_else(|| self.landing_under(ctx).map(|s| s.node)) else {
            return false;
        };
        let (models, other): (Vec<PathBuf>, Vec<PathBuf>) =
            paths.iter().cloned().partition(|p| is_model(p));
        match node {
            // Модель на папке — деталь в ней, остальное — обычный файл.
            Node::Folder(folder) => {
                self.start_adding_in(models, folder.clone());
                if !other.is_empty() {
                    self.copy_into_folder(ctx, folder, other);
                }
            }
            Node::Part(part) => {
                self.start_adding(models, Some(part.clone()));
                self.add_files_from(ctx, part, other);
            }
            // `landing` уже увёл файл в его папку.
            Node::File(_) => return false,
        }
        true
    }

    /// Копии файлов — в папку-группу и сразу в историю.
    fn copy_into_folder(&mut self, ctx: &egui::Context, folder: String, files: Vec<PathBuf>) {
        let Some(project) = self.tab.project.clone() else {
            return;
        };
        self.tab.place = folder.clone();
        self.unfold_to(&folder);
        self.tab.jobs.spawn(ctx, move |_| {
            let added = files
                .iter()
                .map(|f| project.add_folder_file(&folder, f))
                .collect();
            Done::FolderFilesAdded(added)
        });
    }

    /// Открыть обычный файл папки или (`folder`) показать его в папке.
    pub(super) fn open_loose(&mut self, ctx: &egui::Context, path: String, folder: bool) {
        let Some(project) = self.tab.project.clone() else {
            return;
        };
        let way = if folder {
            OpenWay::Folder
        } else {
            OpenWay::Default
        };
        self.tab.jobs.spawn(ctx, move |_| {
            let opened = project.open_folder_file(&path, &way);
            Done::FileOpened(Pick::Current, folder, opened)
        });
    }

    /// Файлы легли в папку: сказать какие, ошибки — тоже.
    pub(super) fn folder_files_added(
        &mut self,
        ctx: &egui::Context,
        added: Vec<anyhow::Result<String>>,
    ) {
        let t = texts();
        let (done, failed): (Vec<_>, Vec<_>) = added.into_iter().partition(|r| r.is_ok());
        let done: Vec<String> = done.into_iter().filter_map(Result::ok).collect();
        let failed: Vec<String> = failed
            .into_iter()
            .filter_map(|r| r.err().map(|e| format!("{e:#}")))
            .collect();
        if !done.is_empty() {
            self.tab.notice = Some(format!("{}: {}", t.files_copied, done.join(", ")));
        }
        if !failed.is_empty() {
            self.tab.message = Some(format!("{}: {}", t.error, failed.join("; ")));
        }
        self.reload(ctx);
    }
}
