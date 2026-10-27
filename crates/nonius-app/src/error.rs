//! Ошибки действий программы. Тексты — для механика: что случилось и что делать.

use std::path::PathBuf;

use nonius_core::PartName;

/// Результат действий программы.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Ошибка действия программы.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Ошибка ядра: файлы проекта, схемы, имена, модули.
    #[error(transparent)]
    Core(#[from] nonius_core::Error),

    /// Ошибка хранилища версий (git).
    #[error(transparent)]
    Store(#[from] nonius_store::StoreError),

    /// Ошибка чтения или записи файла.
    #[error("{path}: {source}")]
    Io {
        /// Файл или папка.
        path: PathBuf,
        /// Исходная ошибка.
        #[source]
        source: std::io::Error,
    },

    /// Нет такой детали.
    #[error("нет детали «{0}» (список: nonius list)")]
    UnknownItem(String),

    /// Файл для импорта не найден.
    #[error("{0}: файл не найден")]
    FileNotFound(PathBuf),

    /// Формат файла модели не поддерживается.
    #[error("{0}: неизвестный формат (поддерживаются STEP, SLDPRT, SLDASM)")]
    UnknownFormat(PathBuf),

    /// Имя детали, взятое из имени файла, не подходит.
    #[error("имя детали из имени файла не подходит ({0}), задайте его: --part <имя>")]
    BadItemName(#[source] nonius_core::Error),

    /// У детали ещё нет версий, сравнивать не с чем.
    #[error("у «{0}» ещё нет версий — сравнивать не с чем")]
    NoVersions(PartName),

    /// Программа собрана без отрисовщика изменений.
    #[error("цветная подсветка доступна в сборке Nonius с OpenCascade")]
    NoRenderer,

    /// Плотность задана неверно.
    #[error("плотность {0} кг/м³ не подходит: нужно число больше нуля")]
    BadDensity(f64),

    /// Расчёт отменён.
    #[error("расчёт отменён")]
    Cancelled,
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
