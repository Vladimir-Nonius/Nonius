//! Действия программы Nonius — общие для командной строки, окна и MCP.
//!
//! Командная строка, окно и ИИ вызывают одни и те же функции [`App`]: открыть
//! проект, список деталей со статусом, история, сохранить версию, импорт,
//! паспорт с массой, сравнение с кэшем. Здесь нет ни печати, ни окон: функции
//! возвращают данные, а показывает их тот, кто вызвал.
//!
//! Модули (OpenCascade, заголовок STEP, …) сюда не зашиты: реестр собирает
//! вызывающий и передаёт в [`App::open`].
//!
//! Файлы: здесь — [`App`] и создание проекта, `items.rs` — детали и статус,
//! `history.rs` — версии, `files.rs` — файлы версии и их открытие,
//! `system/` — программы системы, запуск, показ в папке,
//! `import.rs` — импорт модели, `passport.rs` — паспорт и масса,
//! `materials.rs` — справочник плотностей, `compare.rs` — сравнение,
//! `model_view.rs` — показ детали без сравнения, `cache.rs` — кэш сравнений,
//! `error.rs` — ошибки.

mod cache;
mod compare;
mod error;
mod files;
mod history;
mod import;
mod items;
mod materials;
mod model_view;
mod passport;
mod system;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use nonius_core::extension::Registry;
use nonius_core::project::Item;
use nonius_core::Project;
use nonius_store::Store;

pub use compare::{CompareOptions, Comparison, NoProgress, Progress};
pub use error::{Error, Result, ERROR_CODES};
pub use import::{Imported, PREVIEW_FORMAT};
pub use items::{ItemState, ItemStatus};
pub use materials::{find_material, Material, MATERIALS};
pub use passport::{Mass, MaterialChoice, Passport};
pub use system::{programs_for, OpenWay, Program};

// Типы хранилища, которые возвращают действия: вызывающему не нужен nonius-store.
pub use nonius_store::workdir::work_dir;
pub use nonius_store::{
    is_text_file, readiness, set_author, AiActor, DiffLine, DiffLineKind, FileChange,
    FileChangeKind, GitOptions, Readiness, StoreError, VersionFile, VersionInfo,
};

/// Открытый проект и подключённые модули.
///
/// `App` можно делить между потоками (`Send + Sync`): окно вызывает долгие
/// действия в фоновом потоке. Хранилище версий (git) открывается при первом
/// обращении, поэтому список деталей работает и без git.
pub struct App {
    project: Project,
    registry: Registry,
    git: GitOptions,
    store: OnceLock<Store>,
}

impl App {
    /// Найти проект в папке `start` или выше и открыть его.
    pub fn open(start: &Path, registry: Registry) -> Result<Self> {
        Ok(Self::with_project(Project::discover(start)?, registry))
    }

    /// Открыть уже найденный проект.
    pub fn with_project(project: Project, registry: Registry) -> Self {
        Self {
            project,
            registry,
            git: GitOptions::default(),
            store: OnceLock::new(),
        }
    }

    /// Задать автора версий вместо настроек git: для тестов, сервера и ИИ.
    pub fn with_git_options(mut self, git: GitOptions) -> Self {
        self.git = git;
        self.store = OnceLock::new();
        self
    }

    /// Проект.
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Подключённые модули.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Хранилище версий. Открывается при первом вызове; если git-репозитория
    /// ещё нет, создаёт его.
    pub fn store(&self) -> Result<&Store> {
        if let Some(s) = self.store.get() {
            return Ok(s);
        }
        let opened = Store::open(self.project.clone(), &self.git)?;
        Ok(self.store.get_or_init(|| opened))
    }

    /// Найти деталь или сборку по имени.
    pub fn item(&self, name: &str) -> Result<Item> {
        let parsed = nonius_core::PartName::new(name)?;
        self.project
            .find_item(&parsed)?
            .ok_or_else(|| Error::UnknownItem(name.to_string()))
    }
}

/// Что получилось при создании проекта.
#[derive(Debug, Clone, PartialEq)]
pub struct Created {
    /// Название проекта.
    pub name: String,
    /// Папка проекта.
    pub root: PathBuf,
    /// Как будет храниться история.
    pub history: History,
}

/// Состояние истории версий нового проекта.
#[derive(Debug, Clone, PartialEq)]
pub enum History {
    /// git + Git LFS: всё как задумано.
    Lfs,
    /// Git LFS не найден: модели хранятся прямо в git.
    NoLfs,
    /// История не включена (например, нет git); текст — причина.
    Off(String),
}

/// Создать проект в папке `root`. Название по умолчанию — имя папки.
/// Сразу включает историю версий (git + LFS), если получается.
pub fn create_project(root: &Path, name: Option<&str>) -> Result<Created> {
    let full = std::path::absolute(root).map_err(|e| Error::io(root, e))?;
    if let Some(outer) = full.parent().and_then(|p| Project::discover(p).ok()) {
        return Err(Error::InsideProject {
            name: outer.config().name.clone(),
            root: outer.root().to_path_buf(),
        });
    }
    let name = match name {
        Some(n) => n.to_string(),
        None => root
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "Проект".into()),
    };
    let p = Project::init(root, &name)?;
    let created = |history| Created {
        name: p.config().name.clone(),
        root: p.root().to_path_buf(),
        history,
    };
    Ok(match Store::open(p.clone(), &GitOptions::default()) {
        Ok(s) if s.lfs_enabled() => created(History::Lfs),
        Ok(_) => created(History::NoLfs),
        Err(e) => created(History::Off(e.to_string())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_can_be_shared_between_threads() {
        fn shared<T: Send + Sync>() {}
        shared::<App>();
    }
}
