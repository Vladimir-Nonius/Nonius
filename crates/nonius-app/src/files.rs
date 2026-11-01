//! Файлы детали: что в версии, что изменилось между версиями, открыть файл.
//!
//! Версия `None` — то, что сейчас лежит в папке детали. Файл сохранённой версии
//! открывается копией во временной папке: служебный кэш версий трогать нельзя.

use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::VersionNumber;
use nonius_store::{DiffLine, VersionFile, VersionInfo};

use crate::{App, Error, Result};

/// Папка копий внутри временной папки системы.
const COPIES_DIR: &str = "nonius-versions";

impl App {
    /// Файлы версии `to` и что изменилось после `from`. Без `from` — только
    /// список файлов `to`.
    pub fn version_files(
        &self,
        name: &str,
        from: Option<VersionNumber>,
        to: Option<VersionNumber>,
    ) -> Result<Vec<VersionFile>> {
        let item = self.item(name)?;
        let store = self.store()?;
        let (from, to) = (self.pick(name, from)?, self.pick(name, to)?);
        Ok(store.files_between(&item, from.as_ref(), to.as_ref())?)
    }

    /// Строки текстового файла: разница между версиями, а без `from` — весь
    /// файл версии `to`.
    pub fn file_diff(
        &self,
        name: &str,
        from: Option<VersionNumber>,
        to: Option<VersionNumber>,
        path: &str,
    ) -> Result<Vec<DiffLine>> {
        let item = self.item(name)?;
        let (from, to) = (self.pick(name, from)?, self.pick(name, to)?);
        Ok(self
            .store()?
            .text_diff(&item, from.as_ref(), to.as_ref(), path)?)
    }

    /// Путь, по которому можно открыть файл. Текущий файл — настоящий, файл
    /// версии — копия только для чтения: правки в ней не попадут в историю.
    pub fn file_path(
        &self,
        name: &str,
        version: Option<VersionNumber>,
        path: &str,
    ) -> Result<PathBuf> {
        let listed = self.version_files(name, None, version)?;
        if !listed.iter().any(|f| f.path == path) {
            return Err(Error::UnknownFile {
                item: name.to_string(),
                path: path.to_string(),
            });
        }
        let item = self.item(name)?;
        match self.pick(name, version)? {
            None => Ok(join(&item.dir, path)),
            Some(v) => {
                let folder = format!("{name} {} {}", v.version, v.short_commit());
                let dest = join(&std::env::temp_dir().join(COPIES_DIR).join(folder), path);
                // Версия не меняется: копия уже может быть открыта в программе.
                if !dest.is_file() {
                    let bytes = self.store()?.read_file(&item, &v, path)?;
                    write_copy(&dest, &bytes)?;
                }
                Ok(dest)
            }
        }
    }

    /// Открыть файл в программе, которая назначена в системе для этого типа.
    /// Возвращает, что именно открыто.
    pub fn open_file(
        &self,
        name: &str,
        version: Option<VersionNumber>,
        path: &str,
    ) -> Result<PathBuf> {
        let file = self.file_path(name, version, path)?;
        opener::open(&file).map_err(|e| Error::OpenFailed {
            path: file.clone(),
            why: e.to_string(),
        })?;
        Ok(file)
    }

    fn pick(&self, name: &str, version: Option<VersionNumber>) -> Result<Option<VersionInfo>> {
        version.map(|v| self.version(name, v)).transpose()
    }
}

fn join(dir: &Path, path: &str) -> PathBuf {
    path.split('/').fold(dir.to_path_buf(), |p, s| p.join(s))
}

/// Записать копию целиком: недописанный файл не должен выглядеть готовым.
fn write_copy(dest: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    fs::write(&part, bytes).map_err(|e| Error::io(&part, e))?;
    let mut rights = fs::metadata(&part)
        .map_err(|e| Error::io(&part, e))?
        .permissions();
    rights.set_readonly(true);
    fs::set_permissions(&part, rights).map_err(|e| Error::io(&part, e))?;
    fs::rename(&part, dest).map_err(|e| Error::io(dest, e))
}
