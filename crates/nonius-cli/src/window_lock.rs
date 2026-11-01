//! Один проект — одно окно: два окна спотыкались бы о замки git и затирали бы
//! друг другу картину. Команды и ИИ не запираются: им хватает замков git.
//!
//! Замок — файл `.git/nonius/window.lock`, открытый только этим процессом.
//! Упавшая программа его не оставляет: система закрывает файл сама.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use anyhow::Result;

/// Замки этого процесса: окно может открыть тот же проект заново.
static HELD: Mutex<Vec<(PathBuf, Weak<Held>)>> = Mutex::new(Vec::new());

/// Открытый файл замка.
struct Held {
    _file: Option<std::fs::File>,
}

/// Проект занят этим процессом, пока замок жив.
pub struct WindowLock {
    _held: Arc<Held>,
}

impl WindowLock {
    /// Занять проект в папке `root`.
    pub fn take(root: &Path) -> Result<Self> {
        let mut held = HELD.lock().unwrap_or_else(PoisonError::into_inner);
        held.retain(|(_, w)| w.strong_count() > 0);
        if let Some(known) = held
            .iter()
            .find(|(p, _)| p == root)
            .and_then(|(_, w)| w.upgrade())
        {
            return Ok(Self { _held: known });
        }
        let lock = Arc::new(Held {
            _file: open_lock_file(root)?,
        });
        held.push((root.to_path_buf(), Arc::downgrade(&lock)));
        Ok(Self { _held: lock })
    }
}

/// Без `.git` запирать негде: это проект без истории.
#[cfg(windows)]
fn open_lock_file(root: &Path) -> Result<Option<std::fs::File>> {
    use std::os::windows::fs::OpenOptionsExt;

    let dir = nonius_app::work_dir(root);
    if !root.join(".git").is_dir() {
        return Ok(None);
    }
    std::fs::create_dir_all(&dir)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(dir.join("window.lock"));
    match file {
        Ok(f) => Ok(Some(f)),
        // ERROR_SHARING_VIOLATION: файл держит другой процесс.
        Err(e) if e.raw_os_error() == Some(32) => {
            anyhow::bail!("этот проект уже открыт в другом окне Nonius")
        }
        Err(e) => Err(e.into()),
    }
}

/// Вне Windows программа не выпускается: проект не запирается.
#[cfg(not(windows))]
fn open_lock_file(_root: &Path) -> Result<Option<std::fs::File>> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_process_may_reopen_a_project() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        let first = WindowLock::take(tmp.path()).unwrap();
        let second = WindowLock::take(tmp.path()).unwrap();
        drop(first);
        drop(second);
        WindowLock::take(tmp.path()).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn other_holder_is_refused_in_words() {
        use std::os::windows::fs::OpenOptionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let dir = nonius_app::work_dir(tmp.path());
        std::fs::create_dir_all(&dir).unwrap();
        let _other = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(dir.join("window.lock"))
            .unwrap();
        let e = WindowLock::take(tmp.path()).err().unwrap();
        assert!(e.to_string().contains("другом окне"), "{e}");
    }
}
