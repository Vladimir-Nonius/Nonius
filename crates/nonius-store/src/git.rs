//! Запуск системного `git`.
//!
//! Используется программа `git`, а не библиотека: она уже умеет Git LFS
//! (фильтры), одинаково ведёт себя у всех и стоит у любого, кто работает с git.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{Result, StoreError};

/// Обёртка над `git -C <корень проекта>`.
#[derive(Debug, Clone)]
pub(crate) struct Git {
    root: PathBuf,
    env: Vec<(String, String)>,
}

impl Git {
    pub(crate) fn new(root: &Path, identity: Option<&(String, String)>) -> Self {
        let mut env = Vec::new();
        if let Some((name, email)) = identity {
            for k in ["GIT_AUTHOR_NAME", "GIT_COMMITTER_NAME"] {
                env.push((k.to_string(), name.clone()));
            }
            for k in ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"] {
                env.push((k.to_string(), email.clone()));
            }
        }
        Self {
            root: root.to_path_buf(),
            env,
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new("git");
        c.arg("-C")
            .arg(&self.root)
            .args(["-c", "core.quotepath=false"])
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &self.env {
            c.env(k, v);
        }
        c
    }

    /// Выполнить команду и вернуть stdout.
    pub(crate) fn run(&self, args: &[&str]) -> Result<Vec<u8>> {
        self.run_with_input(args, None)
    }

    /// Выполнить команду, передав `input` на stdin.
    pub(crate) fn run_with_input(&self, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>> {
        let mut cmd = self.command(args);
        if input.is_some() {
            cmd.stdin(Stdio::piped());
        }
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::GitMissing
            } else {
                StoreError::Io {
                    path: self.root.clone(),
                    source: e,
                }
            }
        })?;
        if let (Some(data), Some(mut stdin)) = (input, child.stdin.take()) {
            stdin.write_all(data).map_err(|e| StoreError::Io {
                path: self.root.clone(),
                source: e,
            })?;
        }
        let out = child.wait_with_output().map_err(|e| StoreError::Io {
            path: self.root.clone(),
            source: e,
        })?;
        if out.status.success() {
            return Ok(out.stdout);
        }
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if stderr.contains("Please tell me who you are")
            || stderr.contains("unable to auto-detect email address")
        {
            return Err(StoreError::Identity);
        }
        Err(StoreError::Git {
            args: args.join(" "),
            stderr,
        })
    }

    /// Команда прошла успешно (для проверок вида «есть ли HEAD»).
    pub(crate) fn ok(&self, args: &[&str]) -> bool {
        self.command(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Путь внутри проекта в виде, который понимает git: через `/`.
pub(crate) fn git_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Pathspec без масок: `*`, `?`, `[` в имени воспринимаются буквально.
pub(crate) fn literal(path: &str) -> String {
    format!(":(literal){path}")
}
