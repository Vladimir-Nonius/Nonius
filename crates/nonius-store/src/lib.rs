//! Хранилище версий Nonius на git + Git LFS.
//!
//! Каждая версия детали — коммит git, который меняет только папку этой детали.
//! В сообщении коммита — описание версии и служебные строки:
//!
//! ```text
//! кронштейн v3: толщина 4 → 5 мм под новый вал
//!
//! Подробности для истории.
//!
//! Nonius-Item: кронштейн
//! Nonius-Version: v3
//! ```
//!
//! Поэтому историю можно смотреть и обычным `git log`, а бинарные файлы
//! (STEP, SLDPRT, PDF) хранятся через Git LFS по `.gitattributes` проекта.
//! Ничего не удаляется: откат создаёт новую версию.
//!
//! Файлы: здесь — открытие хранилища, `history.rs` — версии, фиксация и
//! откат, `files.rs` — файлы версии и снимки, `message.rs` — сообщение
//! коммита, `version.rs` — типы версий, `git.rs` — запуск git.

mod error;
mod files;
mod git;
mod history;
mod message;
mod version;

use nonius_core::project::Item;
use nonius_core::{PartName, Project};

pub use error::{Result, StoreError};
use git::{git_path, Git};
pub use version::{FileChange, FileChangeKind, VersionInfo};

/// Настройки доступа к git.
#[derive(Debug, Clone, Default)]
pub struct GitOptions {
    /// Имя и почта автора версий. Если не заданы — берутся из настроек git.
    pub identity: Option<(String, String)>,
}

/// Хранилище версий проекта.
pub struct Store {
    project: Project,
    git: Git,
    lfs: bool,
}

impl Store {
    /// Открыть хранилище проекта. Если git-репозитория ещё нет — создаёт его
    /// и фиксирует настройки проекта первым коммитом.
    pub fn open(project: Project, options: &GitOptions) -> Result<Self> {
        let root = project.root().to_path_buf();
        let git = Git::new(&root, options.identity.as_ref());

        if !root.join(".git").exists() {
            git.run(&["init", "-q", "-b", "main"])?;
        }
        let lfs = git.ok(&["lfs", "version"]);
        if lfs {
            git.run(&["lfs", "install", "--local"])?;
        }

        let store = Self { project, git, lfs };
        if !store.git.ok(&["rev-parse", "-q", "--verify", "HEAD"]) {
            store.initial_commit()?;
        }
        Ok(store)
    }

    fn initial_commit(&self) -> Result<()> {
        let files: Vec<&str> = ["project.yaml", ".gitattributes", "AGENTS.md"]
            .into_iter()
            .filter(|f| self.project.root().join(f).exists())
            .collect();
        let mut add = vec!["add", "--"];
        add.extend(&files);
        self.git.run(&add)?;
        let message = format!(
            "Проект «{}» создан\n\nСоздано Nonius.\n",
            self.project.config().name
        );
        self.git.run_with_input(
            &["commit", "-q", "--allow-empty", "-F", "-"],
            Some(message.as_bytes()),
        )?;
        Ok(())
    }

    /// Проект.
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Включён ли Git LFS. Без него бинарные файлы хранятся прямо в git —
    /// работает, но репозиторий быстрее растёт.
    pub fn lfs_enabled(&self) -> bool {
        self.lfs
    }

    /// Найти деталь или сборку по имени.
    pub fn item(&self, name: &str) -> Result<Item> {
        let parsed = PartName::new(name)?;
        self.project
            .find_item(&parsed)?
            .ok_or_else(|| StoreError::UnknownItem(name.to_string()))
    }

    fn pathspec(&self, item: &Item) -> String {
        git_path(self.project.root(), &item.dir)
    }
}
