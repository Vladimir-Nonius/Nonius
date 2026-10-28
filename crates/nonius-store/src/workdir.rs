//! Служебная папка программы `.git/nonius` и запись в неё целыми папками.
//!
//! Выложенные версии и кэш сравнений готовятся во временной папке рядом и
//! ставятся на место одним переименованием: недописанных папок не бывает,
//! а окно и командная строка могут писать одно и то же одновременно.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Временные папки старше этого срока остались от прерванной программы.
const STALE_TMP: Duration = Duration::from_secs(3600);

/// Служебная папка программы в проекте `root`. В историю не попадает.
pub fn work_dir(root: &Path) -> PathBuf {
    root.join(".git").join("nonius")
}

/// Новая пустая временная папка для будущей папки `dir`: своя у каждого
/// процесса и потока. Брошенные временные папки той же `dir` убираются.
pub fn tmp_dir_for(dir: &Path) -> io::Result<PathBuf> {
    let parent = dir.parent().unwrap_or(dir);
    let name = dir.file_name().unwrap_or_default().to_string_lossy();
    fs::create_dir_all(parent)?;
    remove_stale(parent, &format!("{name}.tmp"));
    let tmp = parent.join(format!(
        "{name}.tmp-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;
    Ok(tmp)
}

/// Поставить готовую папку `tmp` на место `dir`.
///
/// Если на месте уже годная папка (`ready`) — её записал другой поток или
/// процесс, она такая же: оставляем её, `tmp` убираем. Негодную старую
/// отодвигаем во временное имя (её уберёт [`tmp_dir_for`] позже), а не
/// стираем на месте: кто-то мог как раз читать файлы по этому пути.
pub fn publish_dir(tmp: &Path, dir: &Path, mut ready: impl FnMut(&Path) -> bool) -> io::Result<()> {
    if dir.exists() {
        if ready(dir) {
            let _ = fs::remove_dir_all(tmp);
            return Ok(());
        }
        let aside = PathBuf::from(format!("{}-old", tmp.display()));
        let _ = fs::rename(dir, aside);
    }
    match fs::rename(tmp, dir) {
        Ok(()) => Ok(()),
        Err(_) if ready(dir) => {
            let _ = fs::remove_dir_all(tmp);
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_dir_all(tmp);
            Err(e)
        }
    }
}

/// Убрать в `parent` папки, имя которых начинается с `prefix`, старше часа.
fn remove_stale(parent: &Path, prefix: &str) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for e in entries.flatten() {
        if !e.file_name().to_string_lossy().starts_with(prefix) {
            continue;
        }
        let stale = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > STALE_TMP);
        if stale {
            let _ = fs::remove_dir_all(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_keeps_ready_and_replaces_broken() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("a");
        let ready = |d: &Path| d.join("ok").is_file();

        let t = tmp_dir_for(&dir).unwrap();
        fs::write(t.join("ok"), "1").unwrap();
        publish_dir(&t, &dir, ready).unwrap();
        assert_eq!(fs::read_to_string(dir.join("ok")).unwrap(), "1");

        // Годная уже есть — новая не нужна.
        let t = tmp_dir_for(&dir).unwrap();
        fs::write(t.join("ok"), "2").unwrap();
        publish_dir(&t, &dir, ready).unwrap();
        assert_eq!(fs::read_to_string(dir.join("ok")).unwrap(), "1");
        assert!(!t.exists());

        // Негодная заменяется.
        fs::remove_file(dir.join("ok")).unwrap();
        let t = tmp_dir_for(&dir).unwrap();
        fs::write(t.join("ok"), "3").unwrap();
        publish_dir(&t, &dir, ready).unwrap();
        assert_eq!(fs::read_to_string(dir.join("ok")).unwrap(), "3");
    }
}
