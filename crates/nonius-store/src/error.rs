//! Ошибки хранилища версий.

use std::path::PathBuf;

use nonius_core::{PartName, VersionNumber};

/// Результат операций хранилища.
pub type Result<T, E = StoreError> = std::result::Result<T, E>;

/// Ошибка хранилища. Сообщения — для человека: что случилось и что делать.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// Ошибка ядра (формат файлов, имена).
    #[error(transparent)]
    Core(#[from] nonius_core::Error),

    /// Программа git не установлена.
    #[error("не найдена программа git: установите Git (https://git-scm.com) и повторите")]
    GitMissing,

    /// git не знает имя автора.
    #[error(
        "git не знает, кто вы. Выполните один раз:\n  git config --global user.name \"Ваше имя\"\n  git config --global user.email \"почта@example.com\""
    )]
    Identity,

    /// Команда git завершилась с ошибкой.
    #[error("git {args}: {stderr}")]
    Git {
        /// Аргументы команды.
        args: String,
        /// Что ответил git.
        stderr: String,
    },

    /// Ошибка чтения или записи файла.
    #[error("{path}: {source}")]
    Io {
        /// Файл или папка.
        path: PathBuf,
        /// Исходная ошибка.
        #[source]
        source: std::io::Error,
    },

    /// Нет детали с таким именем.
    #[error("нет детали «{0}» (список: nonius list)")]
    UnknownItem(String),

    /// Нет такой версии.
    #[error("у «{item}» нет версии {version} (история: nonius log {item})")]
    UnknownVersion {
        /// Деталь.
        item: PartName,
        /// Версия.
        version: VersionNumber,
    },

    /// Нечего фиксировать.
    #[error("«{0}»: нет изменений с последней версии")]
    NoChanges(PartName),

    /// Есть незафиксированные изменения, а операция их затёрла бы.
    #[error(
        "«{0}»: есть незафиксированные изменения. Сначала зафиксируйте их: nonius commit {0} -m \"...\""
    )]
    Dirty(PartName),

    /// Пустое описание версии.
    #[error("нужно описание версии: -m \"что и почему изменено\"")]
    EmptyMessage,
}
