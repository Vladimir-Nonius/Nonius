//! Что нужно на компьютере для истории версий: git, Git LFS и имя автора.

use std::path::Path;

use crate::git::Git;
use crate::Result;

/// Почта для истории, если человек её не задавал: git требует какую-нибудь.
const NO_EMAIL: &str = "engineer@nonius.local";

/// Что есть на этом компьютере.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Readiness {
    /// Установлен git.
    pub git: bool,
    /// Установлен Git LFS.
    pub lfs: bool,
    /// git знает имя и почту автора версий.
    pub author: bool,
}

/// Проверить компьютер. Переменные окружения git считаются именем автора:
/// так его задают серверы и тесты.
pub fn readiness() -> Readiness {
    let git = Git::new(&std::env::temp_dir(), None);
    let known = |config: &str, env: &str| {
        std::env::var_os(env).is_some() || git.run(&["config", config]).is_ok_and(|o| !o.is_empty())
    };
    Readiness {
        git: git.ok(&["--version"]),
        lfs: git.ok(&["lfs", "version"]),
        author: known("user.name", "GIT_AUTHOR_NAME") && known("user.email", "GIT_AUTHOR_EMAIL"),
    }
}

/// Запомнить имя автора для всех проектов этого компьютера.
pub fn set_author(name: &str) -> Result<()> {
    let git = Git::new(Path::new("."), None);
    git.run(&["config", "--global", "user.name", name.trim()])?;
    if git.run(&["config", "--global", "user.email"]).is_err() {
        git.run(&["config", "--global", "user.email", NO_EMAIL])?;
    }
    Ok(())
}
