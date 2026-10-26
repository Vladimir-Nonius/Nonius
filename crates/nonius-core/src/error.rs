//! Ошибки ядра.

use std::path::PathBuf;

/// Результат операций ядра.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Ошибка ядра. Сообщения пишутся для человека: что случилось и где.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Ошибка чтения или записи файла.
    #[error("{path}: {source}")]
    Io {
        /// Файл или папка.
        path: PathBuf,
        /// Исходная ошибка.
        #[source]
        source: std::io::Error,
    },

    /// Файл YAML не разбирается.
    #[error("{path}: неверный YAML: {source}")]
    Yaml {
        /// Файл.
        path: PathBuf,
        /// Исходная ошибка.
        #[source]
        source: serde_yaml::Error,
    },

    /// Файл записан более новой версией Nonius или неизвестной схемой.
    #[error(
        "{path}: схема {kind} версии {found} не поддерживается (эта программа понимает до {supported}); обновите Nonius"
    )]
    UnsupportedSchema {
        /// Файл.
        path: PathBuf,
        /// Вид файла (`project`, `meta`, `tree`).
        kind: &'static str,
        /// Версия схемы в файле.
        found: u32,
        /// Последняя поддерживаемая версия.
        supported: u32,
    },

    /// Недопустимое имя детали, версии или модуля.
    #[error("недопустимое имя «{name}»: {reason}")]
    InvalidName {
        /// Имя.
        name: String,
        /// Почему не подходит.
        reason: &'static str,
    },

    /// Папка не является проектом Nonius.
    #[error("{0}: здесь нет проекта Nonius (не найден project.yaml)")]
    NotAProject(PathBuf),

    /// Объект уже существует.
    #[error("{0}: уже существует")]
    AlreadyExists(PathBuf),

    /// Модуль нарушил правила подключения или вернул ошибку.
    #[error("модуль {module}: {message}")]
    Module {
        /// Идентификатор модуля.
        module: String,
        /// Описание.
        message: String,
    },
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn module(module: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Module {
            module: module.into(),
            message: message.into(),
        }
    }
}
