//! Что хранилище отдаёт наружу: версия детали и изменённые файлы.

use nonius_core::{PartName, VersionNumber};

/// Зафиксированная версия детали.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    /// Деталь.
    pub item: PartName,
    /// Номер версии.
    pub version: VersionNumber,
    /// Коммит git.
    pub commit: String,
    /// Автор.
    pub author: String,
    /// Дата в формате ISO 8601.
    pub date: String,
    /// Описание версии (без служебных строк).
    pub message: String,
}

impl VersionInfo {
    /// Первая строка описания.
    pub fn summary(&self) -> &str {
        self.message.lines().next().unwrap_or_default()
    }

    /// Короткий хеш коммита.
    pub fn short_commit(&self) -> &str {
        &self.commit[..self.commit.len().min(8)]
    }
}

/// Что изменилось в папке детали с последней версии.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeKind {
    /// Новый файл.
    Added,
    /// Изменённый файл.
    Modified,
    /// Удалённый файл.
    Deleted,
}

/// Изменённый файл детали.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Вид изменения.
    pub kind: FileChangeKind,
    /// Путь внутри папки детали: `model.step`, `docs/чертёж.pdf`.
    pub path: String,
}
