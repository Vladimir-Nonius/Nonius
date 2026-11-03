//! Где лежит деталь: перенос в другую папку и перевод старого проекта
//! (`parts/`, `assemblies/`) на папки. Имя детали при этом не меняется, а в
//! истории остаётся отметка с прежним путём ([`crate::message`]), поэтому
//! старые версии находятся там, где были.

use std::fs;
use std::path::Path;

use nonius_core::project::{Item, ASSEMBLIES_DIR, PARTS_DIR};
use nonius_core::{Error as CoreError, Project};

use crate::git::{git_path, literal};
use crate::message::{layout_message, moved_message};
use crate::{Result, Store, StoreError};

impl Store {
    /// Перенести деталь в папку `folder` (путь от корня, пустой — корень).
    /// Несохранённое не должно смешаться с отметкой, поэтому его сначала надо
    /// сохранить. У детали без версий просто меняется папка.
    pub fn move_item(&self, item: &Item, folder: &Path) -> Result<Item> {
        self.project.writable()?;
        let dir = self.project.folder_dir(folder)?.join(item.name.as_str());
        let moved = Item {
            dir: dir.clone(),
            ..item.clone()
        };
        if dir == item.dir {
            return Ok(moved);
        }
        if dir.starts_with(&item.dir) {
            return Err(CoreError::InsideItem(item.dir.clone()).into());
        }
        if dir.exists() {
            return Err(CoreError::AlreadyExists(dir).into());
        }
        if let Some(parent) = dir.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        if self.latest(item)?.is_none() {
            self.move_dir(&item.name, &item.dir, &dir)?;
            return Ok(moved);
        }
        if !self.changes(item)?.is_empty() {
            return Err(StoreError::Dirty(item.name.clone()));
        }
        let from = self.pathspec(item);
        let to = self.pathspec(&moved);
        let (old, spec) = (literal(&from), literal(&to));
        self.move_dir(&item.name, &item.dir, &dir)?;
        let message = moved_message(&item.name, &from, &to, self.ai.as_ref());
        let done = self
            .git
            .run(&["rm", "-r", "-q", "--cached", "--", &old])
            .and_then(|_| self.git.run(&["add", "-A", "--", &spec]))
            .and_then(|_| self.commit_mark(&message, &[old.clone(), spec.clone()]));
        if let Err(e) = done {
            let _ = self.git.run(&["reset", "-q", "--", &old, &spec]);
            let _ = fs::rename(&dir, &item.dir);
            return Err(e);
        }
        Ok(moved)
    }

    /// Перевести проект старого устройства на папки: детали из `parts/` и
    /// сборки из `assemblies/` переезжают в корень, `project.yaml` получает
    /// схему 2, всё это — одна запись истории. Несохранённое в деталях
    /// переезжает вместе с ними и остаётся несохранённым. Возвращает
    /// обновлённый проект: это хранилище после обновления надо открыть заново.
    pub fn upgrade(&self) -> Result<Project> {
        let mut project = self.project.clone();
        if !project.is_old_layout() {
            return Ok(project);
        }
        let root = project.root().to_path_buf();
        let old: Vec<Item> = project
            .items()?
            .into_iter()
            .filter(|i| {
                i.dir
                    .parent()
                    .is_some_and(|p| p == root.join(PARTS_DIR) || p == root.join(ASSEMBLIES_DIR))
            })
            .collect();
        for item in &old {
            let to = root.join(item.name.as_str());
            if to.exists() {
                return Err(CoreError::AlreadyExists(to).into());
            }
        }
        let mut done: Vec<(&Item, bool)> = Vec::new();
        let result = old.iter().try_for_each(|item| {
            let tracked = self.tracked(&item.dir)?;
            self.relocate(item, &root.join(item.name.as_str()), tracked)?;
            done.push((item, tracked));
            Ok(())
        });
        let result = result
            .and_then(|_| project.finish_upgrade().map_err(StoreError::from))
            .and_then(|_| self.git.run(&["add", "--", "project.yaml"]).map(drop))
            .and_then(|_| {
                let message = layout_message(old.len());
                self.git
                    .run_with_input(&["commit", "-q", "-F", "-"], Some(message.as_bytes()))
                    .map(drop)
            });
        if let Err(e) = result {
            // Всё обратно: детали на старые места, project.yaml как был.
            for (item, tracked) in done.into_iter().rev() {
                let _ = self.relocate_back(item, &root.join(item.name.as_str()), tracked);
            }
            let _ = self.git.run(&["reset", "-q", "--", "project.yaml"]);
            let _ = self.git.run(&["checkout", "-q", "--", "project.yaml"]);
            return Err(e);
        }
        for base in [PARTS_DIR, ASSEMBLIES_DIR] {
            let _ = fs::remove_dir(root.join(base));
        }
        Ok(project)
    }

    /// Есть ли в папке файлы, которые знает git.
    fn tracked(&self, dir: &Path) -> Result<bool> {
        let spec = literal(&git_path(self.project.root(), dir));
        Ok(!self.git.run(&["ls-files", "--", &spec])?.is_empty())
    }

    /// Сдвинуть папку детали: известную git — через `git mv` (в индекс
    /// попадает только перенос, несохранённое остаётся несохранённым).
    fn relocate(&self, item: &Item, to: &Path, tracked: bool) -> Result<()> {
        if !tracked {
            return self.move_dir(&item.name, &item.dir, to);
        }
        let root = self.project.root();
        let (from, to) = (git_path(root, &item.dir), git_path(root, to));
        self.git.run(&["mv", "--", &from, &to])?;
        Ok(())
    }

    fn relocate_back(&self, item: &Item, from: &Path, tracked: bool) -> Result<()> {
        let back = Item {
            dir: from.to_path_buf(),
            ..item.clone()
        };
        self.relocate(&back, &item.dir, tracked)
    }
}
