//! История версий: список, изменения, фиксация, откат.

use std::fs;

use nonius_core::project::{Item, DOCS_DIR};
use nonius_core::VersionNumber;

use crate::git::literal;
use crate::message::{commit_message, parse_record};
use crate::{FileChange, FileChangeKind, Result, Store, StoreError, VersionInfo};

impl Store {
    /// Все версии детали, новые первыми.
    pub fn versions(&self, item: &Item) -> Result<Vec<VersionInfo>> {
        let path = self.pathspec(item);
        let out = self.git.run(&[
            "log",
            "-z",
            "--format=%H%x1f%an%x1f%aI%x1f%B",
            "--",
            &literal(&path),
        ])?;
        let text = String::from_utf8_lossy(&out);
        Ok(text
            .split('\0')
            .filter_map(|record| parse_record(record, &item.name))
            .collect())
    }

    /// Последняя версия детали.
    pub fn latest(&self, item: &Item) -> Result<Option<VersionInfo>> {
        Ok(self.versions(item)?.into_iter().next())
    }

    /// Найти версию по номеру.
    pub fn version(&self, item: &Item, version: VersionNumber) -> Result<VersionInfo> {
        self.versions(item)?
            .into_iter()
            .find(|v| v.version == version)
            .ok_or_else(|| StoreError::UnknownVersion {
                item: item.name.clone(),
                version,
            })
    }

    /// Изменения в папке детали с последней версии.
    pub fn changes(&self, item: &Item) -> Result<Vec<FileChange>> {
        let path = self.pathspec(item);
        let out = self.git.run(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            &literal(&path),
        ])?;
        let text = String::from_utf8_lossy(&out);
        let prefix = format!("{path}/");
        let mut entries = text.split('\0').filter(|e| !e.is_empty());
        let mut changes = Vec::new();
        while let Some(entry) = entries.next() {
            if entry.len() < 4 {
                continue;
            }
            let (code, file) = entry.split_at(3);
            let code = code.as_bytes();
            if matches!(code[0], b'R' | b'C') {
                entries.next(); // старое имя при переименовании
            }
            let kind = if code[0] == b'?' || code[0] == b'A' {
                FileChangeKind::Added
            } else if code[0] == b'D' || code[1] == b'D' {
                FileChangeKind::Deleted
            } else {
                FileChangeKind::Modified
            };
            let rel = file.strip_prefix(&prefix).unwrap_or(file).to_string();
            changes.push(FileChange { kind, path: rel });
        }
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(changes)
    }

    /// Зафиксировать текущее состояние папки детали как новую версию.
    pub fn commit(&self, item: &Item, message: &str) -> Result<VersionInfo> {
        let message = message.trim();
        if message.is_empty() {
            return Err(StoreError::EmptyMessage);
        }
        if self.changes(item)?.is_empty() {
            return Err(StoreError::NoChanges(item.name.clone()));
        }
        let next = match self.latest(item)? {
            Some(v) => v.version.next(),
            None => VersionNumber::FIRST,
        };
        let path = literal(&self.pathspec(item));
        self.git.run(&["add", "-A", "--", &path])?;
        let text = commit_message(&item.name, next, message);
        self.git.run_with_input(
            &["commit", "-q", "-F", "-", "--", &path],
            Some(text.as_bytes()),
        )?;
        self.version(item, next)
    }

    /// Вернуть деталь к версии `version`. Создаёт новую версию с содержимым
    /// старой; история не меняется.
    pub fn restore(
        &self,
        item: &Item,
        version: VersionNumber,
        note: Option<&str>,
    ) -> Result<VersionInfo> {
        if !self.changes(item)?.is_empty() {
            return Err(StoreError::Dirty(item.name.clone()));
        }
        let target = self.version(item, version)?;
        let path = literal(&self.pathspec(item));
        self.git
            .run(&["rm", "-r", "-q", "--ignore-unmatch", "--", &path])?;
        self.git.run(&["checkout", &target.commit, "--", &path])?;
        let docs = item.dir.join(DOCS_DIR);
        fs::create_dir_all(&docs).map_err(|e| StoreError::Io {
            path: docs,
            source: e,
        })?;

        if self.changes(item)?.is_empty() {
            return Err(StoreError::NoChanges(item.name.clone()));
        }
        let mut message = format!("Откат к {version}");
        if let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) {
            message.push_str(": ");
            message.push_str(note);
        }
        message.push_str(&format!(
            "\n\nСодержимое взято из {version} ({}).",
            target.short_commit()
        ));
        self.commit(item, &message)
    }
}
