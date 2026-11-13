//! Обход разбираемой папки: какие файлы в ней есть и какие из них —
//! побайтные копии друг друга. Ни SolidWorks, ни форма тут не нужны.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::units::MODELS;
use super::Found;
use crate::{Error, Result};

/// Снять отпечатки содержимого у моделей, которые могут быть побайтными
/// копиями: то же расширение и тот же размер. Остальные файлы не читаем.
pub(super) fn digests(files: &mut [Found]) {
    use sha2::{Digest, Sha256};

    let mut sizes: HashMap<(String, u64), Vec<usize>> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        if !MODELS.contains(&f.ext.as_str()) {
            continue;
        }
        if let Ok(meta) = fs::metadata(&f.path) {
            sizes
                .entry((f.ext.clone(), meta.len()))
                .or_default()
                .push(i);
        }
    }
    for i in sizes.into_values().filter(|v| v.len() > 1).flatten() {
        let Ok(mut file) = fs::File::open(&files[i].path) else {
            continue;
        };
        let mut hasher = Sha256::new();
        if std::io::copy(&mut file, &mut hasher).is_ok() {
            let hex: String = hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            files[i].digest = Some(hex);
        }
    }
}

/// Обойти папку. Скрытые папки, временные файлы SolidWorks (`~$вал.sldprt`)
/// и чужие проекты Nonius не трогаем.
pub(super) fn walk(dir: &Path, rel: &str, out: &mut Vec<Found>) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::io(dir, e))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.starts_with("~$") {
            continue;
        }
        let kind = entry.file_type().map_err(|e| Error::io(&path, e))?;
        if kind.is_dir() {
            if path.join(nonius_core::project::PROJECT_FILE).is_file() {
                continue;
            }
            let inner = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            walk(&path, &inner, out)?;
        } else if kind.is_file() {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            out.push(Found {
                stem: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                ext: path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default(),
                dir: rel.to_string(),
                modified,
                digest: None,
                path,
            });
        }
    }
    Ok(())
}
