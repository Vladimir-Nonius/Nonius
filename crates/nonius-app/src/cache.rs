//! Кэш результатов сравнения: `.git/nonius/cache/<ключ>/`.
//!
//! Ключ — хеш содержимого обеих версий, допусков и версий модулей. Поменялся
//! файл детали или обновился модуль — ключ другой, и всё считается заново.
//! В папке записи: `cache.yaml` (описание, `schema: N`) и `files/` (картинки,
//! сцены). Запись готовится во временной папке и переименовывается целиком,
//! поэтому недописанных записей не бывает. Незнакомая или битая запись
//! пересчитывается; кэш в историю не попадает и ничего не хранит впервые.

use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::compare::ChangeReport;
use nonius_core::schema::{self, Versioned};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// Описание записи кэша (`cache.yaml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Версия схемы файла.
    pub schema: u32,
    /// Деталь.
    pub item: String,
    /// Подпись «было»: `v3`.
    pub from: String,
    /// Подпись «стало»: `v4` или `текущее`.
    pub to: String,
    /// Модули, которые посчитали запись: `id версия`.
    #[serde(default)]
    pub modules: Vec<String>,
    /// Отчёты сравнения.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<ChangeReport>,
    /// Файлы записи, пути внутри папки записи: `files/highlight-occt/changes.png`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

impl Versioned for CacheEntry {
    const KIND: &'static str = "cache";
    const CURRENT: u32 = 1;
}

/// Имя файла описания записи.
pub(crate) const CACHE_FILE: &str = "cache.yaml";

/// Хеш набора строк — ключ записи.
pub(crate) fn key(parts: &[String]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
    hex(&h.finalize())
}

/// Отпечаток содержимого папки: пути и байты всех файлов.
pub(crate) fn dir_fingerprint(dir: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect(dir, dir, &mut files)?;
    files.sort();
    let mut h = Sha256::new();
    for (rel, path) in files {
        let bytes = fs::read(&path).map_err(|e| Error::io(&path, e))?;
        h.update(rel.as_bytes());
        h.update([0]);
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(&bytes);
    }
    Ok(hex(&h.finalize()))
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    for entry in entries {
        let path = entry.map_err(|e| Error::io(dir, e))?.path();
        if path.is_dir() {
            collect(root, &path, out)?;
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push((rel.to_string_lossy().replace('\\', "/"), path));
        }
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Прочитать готовую запись. Нет записи, она битая, более новой схемы или
/// без своих файлов — `None`: значит, пересчитать.
pub(crate) fn load(dir: &Path) -> Option<CacheEntry> {
    let entry: CacheEntry = schema::load(&dir.join(CACHE_FILE)).ok()?;
    entry
        .files
        .iter()
        .all(|f| dir.join(f).is_file())
        .then_some(entry)
}

/// Записать запись: `fill` кладёт файлы во временную папку и возвращает
/// описание; затем папка целиком переименовывается в `dir`.
pub(crate) fn store(
    dir: &Path,
    fill: impl FnOnce(&Path) -> Result<CacheEntry>,
) -> Result<CacheEntry> {
    let parent = dir.parent().unwrap_or(dir);
    fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    let tmp = parent.join(format!(
        "{}.tmp-{}-{:?}",
        dir.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| Error::io(&tmp, e))?;

    let result = fill(&tmp).and_then(|entry| {
        schema::save(&entry, &tmp.join(CACHE_FILE))?;
        Ok(entry)
    });
    let entry = match result {
        Ok(e) => e,
        Err(e) => {
            let _ = fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };
    if dir.exists() {
        // Битая или чужая запись (её уже признали негодной) — заменить.
        // Если другой поток успел записать годную, берём её.
        if let Some(ready) = load(dir) {
            let _ = fs::remove_dir_all(&tmp);
            return Ok(ready);
        }
        let _ = fs::remove_dir_all(dir);
    }
    if let Err(e) = fs::rename(&tmp, dir) {
        let _ = fs::remove_dir_all(&tmp);
        return load(dir).ok_or_else(|| Error::io(dir, e));
    }
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> CacheEntry {
        CacheEntry {
            schema: CacheEntry::CURRENT,
            item: "кронштейн".into(),
            from: "v1".into(),
            to: "v2".into(),
            modules: vec!["meta-compare 0.1.0".into()],
            reports: vec![ChangeReport::empty("meta-compare")],
            files: vec![],
        }
    }

    #[test]
    fn stores_and_loads() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("abc");
        store(&dir, |_| Ok(entry())).unwrap();
        assert_eq!(load(&dir), Some(entry()));
        let text = fs::read_to_string(dir.join(CACHE_FILE)).unwrap();
        assert!(text.starts_with("schema: 1\n"), "{text}");
    }

    #[test]
    fn broken_newer_or_incomplete_entries_are_misses() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("abc");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CACHE_FILE), "это не yaml: [").unwrap();
        assert_eq!(load(&dir), None);
        fs::write(
            dir.join(CACHE_FILE),
            "schema: 99\nitem: x\nfrom: v1\nto: v2\n",
        )
        .unwrap();
        assert_eq!(load(&dir), None, "более новую схему не читаем");

        let mut e = entry();
        e.files = vec!["files/нет.png".into()];
        schema::save(&e, &dir.join(CACHE_FILE)).unwrap();
        assert_eq!(load(&dir), None, "запись без своих файлов негодна");

        // Негодная запись заменяется новой.
        store(&dir, |_| Ok(entry())).unwrap();
        assert_eq!(load(&dir), Some(entry()));
    }

    #[test]
    fn failed_fill_leaves_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("abc");
        assert!(store(&dir, |_| Err(Error::Cancelled)).is_err());
        assert!(!dir.exists());
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);
    }

    #[test]
    fn fingerprint_follows_content() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("docs")).unwrap();
        fs::write(tmp.path().join("model.step"), "a").unwrap();
        let a = dir_fingerprint(tmp.path()).unwrap();
        assert_eq!(a, dir_fingerprint(tmp.path()).unwrap());
        fs::write(tmp.path().join("docs/x.pdf"), "b").unwrap();
        let b = dir_fingerprint(tmp.path()).unwrap();
        assert_ne!(a, b);
        assert_ne!(key(&["a".into(), "b".into()]), key(&["ab".into()]));
    }
}
