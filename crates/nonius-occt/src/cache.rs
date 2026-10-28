//! Кэш прочитанных STEP в личной папке пользователя (см. [`dir`]).
//!
//! STEP на 5 МБ OpenCascade читает секунд шесть, а сравнение и картинка читают
//! обе версии заново при каждом запуске. Прочитанное тело сохраняется в
//! двоичном формате OpenCascade (BRep): он без потерь и читается за сотые доли
//! секунды. Ключ — SHA-256 содержимого файла и версии OpenCascade, поэтому изменённый
//! файл кэш не обманет. Кэш — только ускорение: если папки нет или файл
//! повреждён, STEP читается как обычно. Хранятся [`MAX_FILES`] тел, которые
//! брали последними.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

/// Сколько тел держать в кэше; старые удаляются.
const MAX_FILES: usize = 64;

/// Меняется, если меняется способ чтения STEP, — старый кэш перестаёт подходить.
const FORMAT: u32 = 1;

/// Папка кэша своя у каждого пользователя: в Windows `%TEMP%` и так личный,
/// в Linux и macOS — `~/.cache/nonius`, чтобы чужой не подложил свои тела.
fn dir() -> PathBuf {
    if cfg!(windows) {
        return std::env::temp_dir().join("nonius-cache");
    }
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")));
    match base {
        Some(base) => base.join("nonius"),
        None => std::env::temp_dir().join("nonius-cache"),
    }
}

/// Файл кэша для STEP с таким содержимым. `None` — файл не прочитать.
pub(crate) fn entry(step: &Path, occt_version: &str) -> Option<PathBuf> {
    let mut file = fs::File::open(step).ok()?;
    let mut h = Sha256::new();
    h.update(FORMAT.to_le_bytes());
    h.update(occt_version.as_bytes());
    h.update([0]);
    io::copy(&mut file, &mut h).ok()?;
    let name: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Some(dir().join(format!("{name}.brep")))
}

fn set_mtime(path: &Path, time: SystemTime) -> io::Result<()> {
    fs::File::options()
        .append(true)
        .open(path)?
        .set_modified(time)
}

/// Отметить, что тело взяли: при чистке уходят те, что не брали дольше всех.
pub(crate) fn touch(entry: &Path) {
    let _ = set_mtime(entry, SystemTime::now());
}

/// Сохранить тело: `write` пишет во временный файл, он переименовывается
/// в `entry`, чтобы другой запуск не прочитал недописанное.
pub(crate) fn store(entry: &Path, write: impl FnOnce(&Path) -> bool) {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let Some(dir) = entry.parent() else {
        return;
    };
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let tmp = dir.join(format!(
        "tmp-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    if write(&tmp) && fs::rename(&tmp, entry).is_ok() {
        trim(dir);
    } else {
        let _ = fs::remove_file(&tmp);
    }
}

/// Оставить в кэше [`MAX_FILES`] тел, которые брали последними, и убрать
/// недописанные файлы, брошенные прерванными запусками (старше часа).
fn trim(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files = Vec::new();
    for e in entries.flatten() {
        let Some(modified) = e.metadata().ok().and_then(|m| m.modified().ok()) else {
            continue;
        };
        let path = e.path();
        if path.extension().is_some_and(|x| x == "brep") {
            files.push((modified, path));
        } else if e.file_name().to_string_lossy().starts_with("tmp-")
            && modified
                .elapsed()
                .is_ok_and(|age| age > Duration::from_secs(3600))
        {
            let _ = fs::remove_file(&path);
        }
    }
    if files.len() <= MAX_FILES {
        return;
    }
    files.sort();
    for (_, path) in &files[..files.len() - MAX_FILES] {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_follows_content_and_version() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.step");
        let b = dir.path().join("b.step");
        fs::write(&a, "ISO-10303-21; одно").unwrap();
        fs::write(&b, "ISO-10303-21; одно").unwrap();
        assert_eq!(entry(&a, "7.9"), entry(&b, "7.9"));
        assert_ne!(entry(&a, "7.9"), entry(&a, "7.8"));
        fs::write(&b, "ISO-10303-21; другое").unwrap();
        assert_ne!(entry(&a, "7.9"), entry(&b, "7.9"));
        assert_eq!(entry(&dir.path().join("нет.step"), "7.9"), None);
    }

    #[test]
    fn store_is_atomic_and_trims_old() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("x.brep");
        store(&target, |_| false);
        assert!(!target.exists());
        store(&target, |tmp| fs::write(tmp, b"brep").is_ok());
        assert_eq!(fs::read(&target).unwrap(), b"brep");
        for i in 0..MAX_FILES + 5 {
            store(&dir.path().join(format!("{i}.brep")), |tmp| {
                fs::write(tmp, b"x").is_ok()
            });
        }
        let left = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(left, MAX_FILES);
    }

    #[test]
    fn touched_entry_survives_trim() {
        let dir = tempfile::tempdir().unwrap();
        let old = SystemTime::now() - Duration::from_secs(7200);
        let keep = dir.path().join("keep.brep");
        fs::write(&keep, b"x").unwrap();
        for i in 0..MAX_FILES {
            let p = dir.path().join(format!("{i}.brep"));
            fs::write(&p, b"x").unwrap();
            set_mtime(&p, old).unwrap();
        }
        let stale = dir.path().join("tmp-1-1");
        fs::write(&stale, b"x").unwrap();
        set_mtime(&stale, old).unwrap();
        touch(&keep);
        store(&dir.path().join("new.brep"), |tmp| {
            fs::write(tmp, b"x").is_ok()
        });
        assert!(keep.exists(), "тело, которое брали, удалено");
        assert!(!stale.exists(), "брошенный недописанный файл остался");
        let left = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(left, MAX_FILES);
    }
}
