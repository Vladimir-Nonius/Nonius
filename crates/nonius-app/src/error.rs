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

    /// Материала нет в справочнике, плотность не задана.
    #[error("«{name}» нет в справочнике: укажите плотность в кг/м³ или выберите материал из справочника{}", if similar.is_empty() { String::new() } else { format!(": {similar}") })]
    UnknownMaterial {
        /// Как назвали материал.
        name: String,
        /// Похожие из справочника через запятую.
        similar: String,
    },

    /// В этой версии детали нет такого файла.
    #[error("в «{item}» нет файла {path}")]
    UnknownFile {
        /// Деталь.
        item: String,
        /// Путь внутри папки детали.
        path: String,
    },

    /// В детали уже есть файл с таким именем.
    #[error("в «{item}» уже есть {path}: переименуйте свой файл или уберите старый")]
    FileExists {
        /// Деталь.
        item: String,
        /// Путь внутри папки детали.
        path: String,
    },

    /// Файл модели или служебный файл нельзя убрать руками.
    #[error("{path} в «{item}» ведёт Nonius: модель меняется новой версией, служебные файлы не убираются")]
    ProtectedFile {
        /// Деталь.
        item: String,
        /// Путь внутри папки детали.
        path: String,
    },

    /// Файл не текстовый: его смотрят программой.
    #[error("{0} не текстовый файл: откройте его программой (nonius open)")]
    NotText(String),

    /// Система не смогла открыть файл.
    #[error("не удалось открыть {path}: {why}")]
    OpenFailed {
        /// Файл.
        path: PathBuf,
        /// Что ответила система.
        why: String,
    },

    /// Расчёт отменён.
    #[error("расчёт отменён")]
    Cancelled,

    /// Сравнить форму можно только с файлом STEP.
    #[error("{0}: сравнить можно только с файлом STEP")]
    NotStep(PathBuf),

    /// Новый проект нельзя создать внутри другого.
    #[error("эта папка уже внутри проекта «{name}» ({root}). Выберите папку вне его")]
    InsideProject {
        /// Название внешнего проекта.
        name: String,
        /// Его папка.
        root: PathBuf,
    },
}

/// Все постоянные коды ошибок и что они значат. Старые коды не меняются и не
/// переиспользуются: на них опираются программы и ИИ.
pub const ERROR_CODES: &[(&str, &str)] = &[
    ("usage", "неверная команда или параметры"),
    ("failed", "ошибка без своего кода, смотрите сообщение"),
    ("no_json", "команда не отвечает в JSON"),
    ("read_only", "сервер MCP запущен без --write"),
    ("unknown_tool", "нет такого инструмента MCP"),
    ("io", "ошибка чтения или записи файла"),
    ("bad_yaml", "файл YAML не разбирается"),
    (
        "unsupported_schema",
        "файл записан более новой версией Nonius",
    ),
    ("invalid_name", "недопустимое имя детали или версии"),
    ("not_a_project", "здесь нет проекта Nonius"),
    ("already_exists", "такая деталь или файл уже есть"),
    ("module", "ошибка модуля"),
    ("git_missing", "не установлен git"),
    ("git_identity", "git не знает имя автора"),
    ("git_failed", "команда git завершилась ошибкой"),
    ("unknown_item", "нет такой детали"),
    ("unknown_version", "нет такой версии"),
    ("no_changes", "нет изменений с последней версии"),
    ("dirty", "есть несохранённые изменения"),
    ("bad_actor", "имя или модель ИИ не годятся"),
    ("empty_message", "нужно описание версии"),
    ("file_not_found", "файл для импорта не найден"),
    ("unknown_format", "неизвестный формат модели"),
    ("no_versions", "у детали ещё нет версий"),
    ("no_renderer", "сборка без подсветки изменений"),
    ("bad_density", "неверная плотность"),
    (
        "unknown_material",
        "материала нет в справочнике, нужна плотность",
    ),
    ("unknown_file", "в версии нет такого файла"),
    ("open_failed", "система не смогла открыть файл"),
    ("cancelled", "расчёт отменён"),
    ("inside_project", "папка уже внутри другого проекта"),
    ("in_use", "файлы детали открыты в другой программе"),
    ("not_deleted", "среди удалённых нет такой детали"),
    ("not_step", "сравнить можно только с файлом STEP"),
    ("not_text", "файл не текстовый"),
    (
        "protected_file",
        "файл модели или служебный файл не убирается",
    ),
];

impl Error {
    /// Постоянный код для программ: не меняется вместе с текстом сообщения.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Core(e) => e.code(),
            Self::Store(e) => e.code(),
            Self::Io { .. } => "io",
            Self::UnknownItem(_) => "unknown_item",
            Self::FileNotFound(_) => "file_not_found",
            Self::UnknownFormat(_) => "unknown_format",
            Self::BadItemName(_) => "invalid_name",
            Self::NoVersions(_) => "no_versions",
            Self::NoRenderer => "no_renderer",
            Self::BadDensity(_) => "bad_density",
            Self::UnknownMaterial { .. } => "unknown_material",
            Self::UnknownFile { .. } => "unknown_file",
            Self::FileExists { .. } => "already_exists",
            Self::ProtectedFile { .. } => "protected_file",
            Self::NotText(_) => "not_text",
            Self::OpenFailed { .. } => "open_failed",
            Self::Cancelled => "cancelled",
            Self::InsideProject { .. } => "inside_project",
            Self::NotStep(_) => "not_step",
        }
    }

    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
