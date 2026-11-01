//! Что хранилище отдаёт наружу: версия детали и изменённые файлы.

use nonius_core::meta::ItemKind;
use nonius_core::{PartName, VersionNumber};

use crate::{Result, StoreError};

/// Почта автора-ИИ в git: у ИИ своей нет, а git требует.
const AI_EMAIL: &str = "ai@nonius.invalid";

/// ИИ, который действует в проекте. Запускает его человек: он остаётся в
/// коммите как committer, а автором версии становится ИИ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiActor {
    name: String,
    model: Option<String>,
}

impl AiActor {
    /// Имя и, если известна, модель. Обе строки попадают в коммит, поэтому
    /// переводы строк и угловые скобки запрещены.
    pub fn new(name: &str, model: Option<&str>) -> Result<Self> {
        let clean = |text: &str| {
            let text = text.trim();
            if text.is_empty() || text.contains(['\n', '\r', '<', '>']) {
                Err(StoreError::BadActor(text.to_string()))
            } else {
                Ok(text.to_string())
            }
        };
        Ok(Self {
            name: clean(name)?,
            model: model.map(clean).transpose()?,
        })
    }

    /// Имя.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Модель, если указана.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Значение `git commit --author`.
    pub(crate) fn git_author(&self) -> String {
        format!("{} <{AI_EMAIL}>", self.name)
    }
}

/// Зафиксированная версия детали.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    /// Деталь: её нынешнее имя, даже если версия сделана под прежним.
    pub item: PartName,
    /// Номер версии.
    pub version: VersionNumber,
    /// Папка детали в этой версии, от корня проекта. После переименования
    /// старые версии лежат в истории под прежним именем.
    pub folder: String,
    /// Коммит git.
    pub commit: String,
    /// Автор: человек или ИИ.
    pub author: String,
    /// Кто запустил фиксацию (committer git). У ИИ это человек.
    pub committer: String,
    /// ИИ, если версию сделал он.
    pub ai: Option<AiActor>,
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

    /// Подпись автора: `Владимир` или
    /// `Claude (ИИ, claude-opus-5-5, запустил Владимир)`.
    pub fn signature(&self) -> String {
        let Some(ai) = &self.ai else {
            return self.author.clone();
        };
        let model = ai.model().map(|m| format!(", {m}")).unwrap_or_default();
        format!("{} (ИИ{model}, запустил {})", ai.name(), self.committer)
    }

    /// Короткий хеш коммита.
    pub fn short_commit(&self) -> &str {
        &self.commit[..self.commit.len().min(8)]
    }
}

/// Удалённая деталь: её файлы и версии остаются в истории.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedItem {
    /// Имя.
    pub name: PartName,
    /// Деталь или сборка.
    pub kind: ItemKind,
    /// Коммит удаления.
    pub commit: String,
    /// Кто удалил.
    pub author: String,
    /// Когда, в формате ISO 8601.
    pub date: String,
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

/// Файл детали при сравнении двух версий.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionFile {
    /// Путь внутри папки детали.
    pub path: String,
    /// Что с ним случилось; `None` — не менялся.
    pub change: Option<FileChangeKind>,
    /// Размер в первой версии, байт; `None` — файла там нет.
    pub before: Option<u64>,
    /// Размер во второй версии, байт; `None` — файла там нет.
    pub after: Option<u64>,
}

/// Вид строки в разнице текста.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    /// Без изменений.
    Same,
    /// Добавлена.
    Added,
    /// Убрана.
    Removed,
    /// Здесь пропущены строки без изменений.
    Gap,
}

/// Строка в разнице текста.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Вид строки.
    pub kind: DiffLineKind,
    /// Текст без значка.
    pub text: String,
}
