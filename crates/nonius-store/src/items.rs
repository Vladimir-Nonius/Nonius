//! Удалить, вернуть и переименовать деталь, убрать версию с карты. Это не
//! версии, а отметки в истории
//! ([`crate::message`]): старое остаётся в git. Папка сдвигается одним шагом до
//! записи в git: если файл держит другая программа, ничего не меняется.

use std::fs;
use std::path::Path;

use nonius_core::project::{old_item_path, Item, DOCS_DIR};
use nonius_core::{Error as CoreError, PartName, VersionNumber};

use crate::git::literal;
use crate::message::{deleted_message, hidden_message, recovered_message, renamed_message};
use crate::{workdir, AiActor, DeletedItem, Result, Store, StoreError};

/// Описание версии, в которой сохраняется несохранённое перед удалением.
const BEFORE_DELETE: &str = "Сохранено перед удалением";

/// Коды Windows, когда файл держит другая программа: нет доступа, файл занят,
/// часть файла заблокирована.
const FILE_HELD: [i32; 3] = [5, 32, 33];

impl Store {
    /// Записать подготовленный индекс в отметку истории.
    pub(crate) fn commit_mark(&self, message: &str, paths: &[String]) -> Result<()> {
        let author = self.ai.as_ref().map(AiActor::git_author);
        let mut args = vec!["commit", "-q", "-F", "-"];
        if let Some(author) = &author {
            args.extend(["--author", author]);
        }
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
        self.git.run_with_input(&args, Some(message.as_bytes()))?;
        Ok(())
    }

    /// Убрать версию с карты (`hidden`) или вернуть на неё. Файлы и версия
    /// остаются в истории; отметка — пустой коммит, чужое в индексе не
    /// попадает.
    pub fn set_hidden(&self, item: &Item, version: VersionNumber, hidden: bool) -> Result<()> {
        self.project.writable()?;
        let v = self.version(item, version)?;
        if v.hidden == hidden {
            return Ok(());
        }
        let message = hidden_message(&item.name, version, hidden, self.ai.as_ref());
        let author = self.ai.as_ref().map(AiActor::git_author);
        let mut args = vec!["commit", "-q", "--allow-empty", "--only", "-F", "-"];
        if let Some(author) = &author {
            args.extend(["--author", author]);
        }
        self.git.run_with_input(&args, Some(message.as_bytes()))?;
        Ok(())
    }

    /// Сдвинуть папку детали целиком.
    pub(crate) fn move_dir(&self, item: &PartName, from: &Path, to: &Path) -> Result<()> {
        fs::rename(from, to).map_err(|e| {
            if cfg!(windows) && e.raw_os_error().is_some_and(|c| FILE_HELD.contains(&c)) {
                StoreError::InUse(item.clone())
            } else {
                StoreError::Io {
                    path: from.to_path_buf(),
                    source: e,
                }
            }
        })
    }

    /// Удалить деталь. Файлы и версии остаются в истории, деталь можно вернуть
    /// ([`Store::recover`]). Несохранённое сначала сохраняется версией: иначе
    /// вернуть его было бы неоткуда.
    pub fn delete(&self, item: &Item) -> Result<()> {
        self.project.writable()?;
        if !self.changes(item)?.is_empty() {
            self.commit(item, BEFORE_DELETE)?;
        }
        let path = self.pathspec(item);
        let aside = workdir::tmp_dir_for(&workdir::work_dir(self.project.root()).join("deleted"))
            .and_then(|tmp| fs::remove_dir(&tmp).map(|_| tmp))
            .map_err(|e| StoreError::Io {
                path: item.dir.clone(),
                source: e,
            })?;
        self.move_dir(&item.name, &item.dir, &aside)?;
        let spec = literal(&path);
        let message = deleted_message(&item.name, item.kind, &path, self.ai.as_ref());
        let done = self
            .git
            .run(&["rm", "-r", "-q", "--cached", "--", &spec])
            .and_then(|_| self.commit_mark(&message, std::slice::from_ref(&spec)));
        if let Err(e) = done {
            let _ = self.git.run(&["reset", "-q", "--", &spec]);
            let _ = fs::rename(&aside, &item.dir);
            return Err(e);
        }
        // Файлы уже в истории; что не убралось сейчас, лежит в служебной папке.
        let _ = fs::remove_dir_all(&aside);
        Ok(())
    }

    /// Вернуть удалённую деталь такой, какой она была перед удалением, туда,
    /// где она лежала. Удалённая до перехода на папки возвращается в корень.
    pub fn recover(&self, name: &PartName) -> Result<Item> {
        self.project.writable()?;
        if let Some(existing) = self.project.find_item(name)? {
            return Err(CoreError::AlreadyExists(existing.dir).into());
        }
        let deleted = self.deleted()?;
        let found: &DeletedItem = deleted
            .iter()
            .find(|d| d.name.same_as(name))
            .ok_or_else(|| StoreError::NotDeleted(name.clone()))?;
        let root = self.project.root();
        let (path, dir, moved) = match &found.folder {
            Some(folder) => (folder.clone(), root.join(folder), None),
            None => {
                let old = old_item_path(found.kind, &found.name);
                (
                    found.name.to_string(),
                    root.join(found.name.as_str()),
                    Some(old),
                )
            }
        };
        if dir.exists() {
            return Err(CoreError::AlreadyExists(dir).into());
        }
        let parent = format!("{}^", found.commit);
        let spec = literal(&path);
        match &moved {
            None => {
                self.git.run(&["checkout", &parent, "--", &spec])?;
            }
            Some(old) => {
                // Файлы встают на старое место, а оттуда — в корень.
                let old_spec = literal(old);
                self.git.run(&["checkout", &parent, "--", &old_spec])?;
                let old_dir = root.join(old);
                let done = self
                    .move_dir(&found.name, &old_dir, &dir)
                    .and_then(|_| {
                        self.git
                            .run(&["rm", "-r", "-q", "--cached", "--", &old_spec])
                    })
                    .and_then(|_| self.git.run(&["add", "-A", "--", &spec]));
                if let Err(e) = done {
                    let _ = self.git.run(&["reset", "-q", "--", &old_spec, &spec]);
                    let _ = fs::remove_dir_all(&old_dir);
                    let _ = fs::remove_dir_all(&dir);
                    return Err(e);
                }
                if let Some(base) = old_dir.parent() {
                    let _ = fs::remove_dir(base);
                }
            }
        }
        let message = recovered_message(&found.name, moved.as_deref(), self.ai.as_ref());
        self.commit_mark(&message, &[spec])?;
        let docs = dir.join(DOCS_DIR);
        fs::create_dir_all(&docs).map_err(|e| StoreError::Io {
            path: docs,
            source: e,
        })?;
        Ok(Item {
            name: found.name.clone(),
            kind: found.kind,
            dir,
        })
    }

    /// Переименовать деталь: папка получает новое имя, версии остаются при
    /// ней. Несохранённое не должно смешаться с отметкой, поэтому его сначала
    /// надо сохранить. У детали без версий просто меняется папка.
    pub fn rename(&self, item: &Item, new: &PartName) -> Result<Item> {
        self.project.writable()?;
        if let Some(other) = self.project.find_item(new)? {
            if other.dir != item.dir {
                return Err(CoreError::AlreadyExists(other.dir).into());
            }
        }
        let renamed = Item {
            name: new.clone(),
            kind: item.kind,
            dir: item.dir.with_file_name(new.as_str()),
        };
        if *new == item.name {
            return Ok(renamed);
        }
        if self.latest(item)?.is_none() {
            self.move_dir(&item.name, &item.dir, &renamed.dir)?;
            return Ok(renamed);
        }
        if !self.changes(item)?.is_empty() {
            return Err(StoreError::Dirty(item.name.clone()));
        }
        let old = literal(&self.pathspec(item));
        let spec = literal(&self.pathspec(&renamed));
        self.move_dir(&item.name, &item.dir, &renamed.dir)?;
        let message = renamed_message(&item.name, new, self.ai.as_ref());
        // Сначала убрать старые пути: иначе на Windows смена регистра не видна.
        let done = self
            .git
            .run(&["rm", "-r", "-q", "--cached", "--", &old])
            .and_then(|_| self.git.run(&["add", "-A", "--", &spec]))
            .and_then(|_| self.commit_mark(&message, &[old.clone(), spec.clone()]));
        if let Err(e) = done {
            let _ = self.git.run(&["reset", "-q", "--", &old, &spec]);
            let _ = fs::rename(&renamed.dir, &item.dir);
            return Err(e);
        }
        Ok(renamed)
    }
}
