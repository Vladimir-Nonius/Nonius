//! Файлы детали: что в версии, что изменилось между версиями, открыть файл,
//! положить свой файл в `docs/` и убрать файл из папки детали.
//!
//! Версия `None` — то, что сейчас лежит в папке детали. Файл сохранённой версии
//! открывается копией во временной папке: служебный кэш версий трогать нельзя.

use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::project::{DOCS_DIR, META_FILE, TREE_FILE};
use nonius_core::VersionNumber;
use nonius_store::{is_text_file, DiffLine, VersionFile, VersionInfo};

use crate::system::{self, OpenWay};
use crate::{App, Error, Result};

/// Папка копий внутри временной папки системы.
const COPIES_DIR: &str = "nonius-versions";

/// Файл модели детали: сначала исходник CAD, потом STEP.
const MODEL_FILES: [&str; 3] = ["model.sldasm", "model.sldprt", "model.step"];

/// Файлы, которые ведёт сам Nonius: их не удаляют руками.
const SERVICE_FILES: [&str; 3] = [META_FILE, TREE_FILE, "preview.png"];

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
        self.ensure_listed(name, version, path)?;
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

    /// Весь текстовый файл детали: как сейчас в папке или в версии.
    pub fn file_text(
        &self,
        name: &str,
        version: Option<VersionNumber>,
        path: &str,
    ) -> Result<String> {
        if !is_text_file(path) {
            return Err(Error::NotText(path.to_string()));
        }
        self.ensure_listed(name, version, path)?;
        let lines = self.file_diff(name, None, version, path)?;
        let lines: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        Ok(lines.join("\n"))
    }

    /// Такой файл есть в версии: путь не уводит из папки детали.
    fn ensure_listed(&self, name: &str, version: Option<VersionNumber>, path: &str) -> Result<()> {
        let listed = self.version_files(name, None, version)?;
        if listed.iter().any(|f| f.path == path) {
            return Ok(());
        }
        Err(Error::UnknownFile {
            item: name.to_string(),
            path: path.to_string(),
        })
    }

    /// Файл модели детали сейчас: его открывает правый щелчок по детали.
    pub fn model_file(&self, name: &str) -> Result<Option<String>> {
        let item = self.item(name)?;
        let found = MODEL_FILES.iter().find(|f| item.dir.join(f).is_file());
        Ok(found.map(|f| f.to_string()))
    }

    /// Открыть файл программой или показать в папке. Возвращает, что именно
    /// открыто.
    pub fn open_file(
        &self,
        name: &str,
        version: Option<VersionNumber>,
        path: &str,
        way: &OpenWay,
    ) -> Result<PathBuf> {
        let file = self.file_path(name, version, path)?;
        system::open(&file, way).map_err(|e| Error::OpenFailed {
            path: file.clone(),
            why: e.to_string(),
        })?;
        Ok(file)
    }

    /// Положить копию своего файла (технология, чертёж, ТЗ) в `docs/` детали.
    /// Файл с таким именем не перезаписывается. Возвращает путь внутри детали;
    /// в историю файл попадёт со следующей версией.
    pub fn add_file(&self, name: &str, file: &Path) -> Result<String> {
        let item = self.item(name)?;
        let file_name = match file.file_name() {
            Some(n) if file.is_file() => n.to_string_lossy().into_owned(),
            _ => return Err(Error::FileNotFound(file.to_path_buf())),
        };
        let path = format!("{DOCS_DIR}/{file_name}");
        let dest = join(&item.dir, &path);
        if dest.exists() {
            return Err(Error::FileExists {
                item: name.to_string(),
                path,
            });
        }
        let docs = item.dir.join(DOCS_DIR);
        fs::create_dir_all(&docs).map_err(|e| Error::io(&docs, e))?;
        fs::copy(file, &dest).map_err(|e| Error::io(&dest, e))?;
        Ok(path)
    }

    /// Убрать файл из папки детали. В сохранённых версиях он остаётся; файл
    /// модели и служебные файлы так не убираются.
    pub fn remove_file(&self, name: &str, path: &str) -> Result<()> {
        let item = self.item(name)?;
        let unknown = || Error::UnknownFile {
            item: name.to_string(),
            path: path.to_string(),
        };
        let parts: Vec<&str> = path.split('/').collect();
        if parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == "..")
        {
            return Err(unknown());
        }
        if MODEL_FILES.contains(&path) || SERVICE_FILES.contains(&path) {
            return Err(Error::ProtectedFile {
                item: name.to_string(),
                path: path.to_string(),
            });
        }
        let file = join(&item.dir, path);
        if !file.is_file() {
            return Err(unknown());
        }
        fs::remove_file(&file).map_err(|e| Error::io(&file, e))?;
        // Пустые папки после файла не нужны: git их всё равно не хранит.
        let mut dir = file.parent();
        while let Some(d) = dir.filter(|d| *d != item.dir) {
            if fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
        Ok(())
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
