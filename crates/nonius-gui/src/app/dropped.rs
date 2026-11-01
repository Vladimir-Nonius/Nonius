//! Перетащили в окно: файлы моделей, папку с моделями или папку проекта.

use std::path::PathBuf;

use eframe::egui;

use super::file_menu::{in_project, is_model, models_in};
use super::App;
use crate::texts::texts;

impl App {
    /// Что-то перетащили в окно.
    pub(super) fn take_dropped(&mut self, ctx: &egui::Context) {
        let paths: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        self.take_paths(ctx, paths);
    }

    /// Положить перетащенное: модели — в проект, папка проекта открывается.
    /// Без открытого проекта модели становятся поводом его создать.
    pub(super) fn take_paths(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>) {
        let Some(first) = paths.first() else {
            return;
        };
        let projects = |p: &&PathBuf| p.is_dir() && in_project(p);
        if let Some(dir) = paths.iter().find(projects) {
            self.open(ctx, dir.clone());
            return;
        }
        let mut models: Vec<PathBuf> = Vec::new();
        for path in &paths {
            if path.is_dir() {
                models.extend(models_in(path));
            } else if is_model(path) {
                models.push(path.clone());
            }
        }
        if models.is_empty() {
            self.message = Some(texts().no_models_there.to_string());
        } else if self.project.is_some() {
            self.start_adding(models, None);
        } else if in_project(first) {
            self.open(ctx, first.parent().unwrap_or(first).to_path_buf());
        } else {
            let dir = if first.is_dir() {
                Some(first.as_path())
            } else {
                first.parent()
            };
            let dir = dir.unwrap_or(first).to_path_buf();
            self.new_here = Some((dir, models));
        }
    }
}
