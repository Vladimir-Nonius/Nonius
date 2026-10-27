//! Кэш прочитанных STEP в папке временных файлов.
//!
//! STEP на 5 МБ OpenCascade читает секунд шесть, а сравнение и картинка читают
//! обе версии заново при каждом запуске. Прочитанное тело сохраняется в
//! двоичном формате OpenCascade (BRep): он без потерь и читается за сотые доли
//! секунды. Ключ — содержимое файла и версия OpenCascade, поэтому изменённый
//! файл кэш не обманет. Кэш — только ускорение: если папки нет или файл
//! повреждён, STEP читается как обычно. Хранятся последние [`MAX_FILES`] тел.

use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Сколько тел держать в кэше; старые удаляются.
const MAX_FILES: usize = 64;

/// Меняется, если меняется способ чтения STEP, — старый кэш перестаёт подходить.
const FORMAT: u32 = 1;

fn dir() -> PathBuf {
    std::env::temp_dir().join("nonius-cache")
}

/// Файл кэша для STEP с таким содержимым. `None` — файл не прочитать.
pub(crate) fn entry(step: &Path, occt_version: &str) -> Option<PathBuf> {
    let bytes = fs::read(step).ok()?;
    let mut h = DefaultHasher::new();
    FORMAT.hash(&mut h);
    occt_version.hash(&mut h);
    bytes.hash(&mut h);
    Some(dir().join(format!("{:x}-{:016x}.brep", bytes.len(), h.finish())))
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

/// Оставить в кэше только последние [`MAX_FILES`] тел.
fn trim(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "brep"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
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
}
