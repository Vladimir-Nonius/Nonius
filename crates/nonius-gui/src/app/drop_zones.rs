//! Куда бросили файл из Проводника. Система отдаёт окну только сами файлы,
//! без места: строки дерева и блоки «Структуры» каждый кадр запоминают, где
//! они на экране, а место броска — курсор в этот миг. Пока файл несут над
//! окном, папка или деталь под курсором подсвечена. Здесь же — открыть
//! обычный файл папки.

use std::path::PathBuf;

use eframe::egui;

use super::file_menu::is_model;
use super::jobs::Done;
use super::App;
use crate::source::{OpenWay, Pick};
use crate::texts::texts;

/// На что можно бросить файл.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Zone {
    /// Папка-группа: путь от корня, пустой — корень.
    Folder(String),
    /// Деталь.
    Part(String),
}

impl App {
    /// Где сейчас курсор в окне: у системы, а без неё — у egui.
    fn pointer_in_window(&self, ctx: &egui::Context) -> Option<egui::Pos2> {
        let Some((x, y)) = self.chooser.cursor() else {
            return ctx.input(|i| i.pointer.latest_pos());
        };
        let ppp = ctx.pixels_per_point();
        let inner = ctx.input(|i| i.viewport().inner_rect)?;
        Some(egui::pos2(x / ppp, y / ppp) - inner.min.to_vec2())
    }

    /// Папка или деталь под курсором. Строки, нарисованные позже, лежат
    /// глубже (деталь внутри папки): они главнее.
    fn zone_under(&self, ctx: &egui::Context) -> Option<(egui::Rect, Zone)> {
        let pos = self.pointer_in_window(ctx)?;
        let zones = &self.tab.zones;
        zones.iter().rev().find(|(r, _)| r.contains(pos)).cloned()
    }

    /// Пока файлы несут над окном — подсветить, куда они лягут.
    pub(super) fn show_drop_target(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.raw.hovered_files.is_empty()) {
            return;
        }
        // Курсор системы двигается без событий окна: смотреть каждый кадр.
        ctx.request_repaint();
        if let Some((rect, _)) = self.zone_under(ctx) {
            let stroke = ctx.style().visuals.selection.stroke;
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop-target"),
            ));
            painter.rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Outside);
        }
    }

    /// Файлы бросили: на папку — копия в неё, на деталь — модель новой
    /// версией, остальное в `docs/`. Мимо дерева — `None`, решает обычный
    /// путь (модели добавить, папку проекта открыть).
    pub(super) fn drop_on_zone(&mut self, ctx: &egui::Context, paths: &[PathBuf]) -> bool {
        if self.tab.project.is_none() || paths.iter().any(|p| p.is_dir()) {
            return false;
        }
        let Some((_, zone)) = self.zone_under(ctx) else {
            return false;
        };
        match zone {
            Zone::Folder(folder) => self.copy_into_folder(ctx, folder, paths.to_vec()),
            Zone::Part(part) => {
                let (models, other): (Vec<PathBuf>, Vec<PathBuf>) =
                    paths.iter().cloned().partition(|p| is_model(p));
                self.start_adding(models, Some(part.clone()));
                self.add_files_from(ctx, part, other);
            }
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
