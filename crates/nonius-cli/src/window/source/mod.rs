//! Мост от общего слоя `nonius-app` к окну: окно не знает про git и
//! OpenCascade, а сравнения берёт из того же кэша, что и команды.
//!
//! Здесь — проект и замки чтения и записи; `compare.rs` — сравнение,
//! `files.rs` — файлы детали, `machine.rs` — git и имя автора, `words.rs` —
//! ошибки и масса словами.

mod compare;
mod files;
mod machine;
mod words;

pub(super) use machine::ThisMachine;
pub(super) use words::plainly;
use words::{in_words, mass_entry};

use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::format::short_date;
use crate::window_lock::WindowLock;
use crate::window_registry;

use anyhow::{bail, Result};
use nonius_app::{App, FileChangeKind, History, MaterialChoice, MATERIALS};
use nonius_core::meta::ItemKind;
use nonius_core::VersionNumber;
use nonius_gui::source::{
    AddedModel, Comparison, DeletedEntry, DiffEntry, FileEntry, MassEntry, MaterialEntry, OpenWay,
    Other, PartEntry, Pick, ProgramEntry, Progress, ProjectSource, VersionEntry,
};

/// Проект для окна поверх `nonius-app`.
pub(super) struct AppSource {
    app: App,
    /// Одно сравнение за раз: OpenCascade падает при параллельном чтении STEP,
    /// а человек может быстро переключить версии, пока идёт прошлое сравнение.
    busy: Mutex<()>,
    /// Запись (сохранить версию, назначить материал) — когда никто не читает
    /// папку детали и git: иначе git спотыкается о свой index.lock, а
    /// сравнение видит полусохранённое.
    files: RwLock<()>,
    /// Держит проект за этим процессом, пока он открыт.
    _lock: WindowLock,
}

impl AppSource {
    pub(super) fn open(path: &Path) -> Result<Box<dyn ProjectSource>> {
        let app = App::open(path, window_registry())?;
        let lock = WindowLock::take(app.project().root())?;
        Ok(Box::new(Self {
            app,
            busy: Mutex::new(()),
            files: RwLock::new(()),
            _lock: lock,
        }))
    }

    /// Создать проект в папке, название — имя папки. Проект без истории
    /// версий окну не нужен: сохранить в нём было бы нельзя.
    pub(super) fn create(path: &Path) -> Result<Box<dyn ProjectSource>> {
        let created = nonius_app::create_project(path, None)?;
        if let History::Off(why) = created.history {
            bail!("проект создан, но история версий не включена: {why}");
        }
        Self::open(path)
    }

    fn reading(&self) -> RwLockReadGuard<'_, ()> {
        self.files.read().unwrap_or_else(|e| e.into_inner())
    }

    fn writing(&self) -> RwLockWriteGuard<'_, ()> {
        self.files.write().unwrap_or_else(|e| e.into_inner())
    }
}

fn version(p: Pick) -> Result<Option<VersionNumber>> {
    Ok(match p {
        Pick::Version(n) => Some(VersionNumber::new(n)?),
        Pick::Current => None,
    })
}

/// Папка, в которой лежит деталь: путь от корня через `/`, пустой — корень.
fn folder_of(root: &Path, dir: &Path) -> String {
    let parent = dir.parent().unwrap_or(root);
    let rel = parent.strip_prefix(root).unwrap_or(Path::new(""));
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Пара версий для вкладки «Файлы»: одна и та же версия — без сравнения.
fn pair(before: Pick, after: Pick) -> Result<(Option<VersionNumber>, Option<VersionNumber>)> {
    let to = version(after)?;
    let from = if before == after {
        None
    } else {
        version(before)?
    };
    Ok((from, to))
}

impl ProjectSource for AppSource {
    fn title(&self) -> String {
        self.app.project().config().name.clone()
    }

    fn root(&self) -> &Path {
        self.app.project().root()
    }

    fn parts(&self) -> Result<Vec<PartEntry>> {
        let _files = self.reading();
        let root = self.app.project().root();
        Ok(self
            .app
            .items()?
            .into_iter()
            .map(|s| PartEntry {
                name: s.item.name.to_string(),
                assembly: s.item.kind == ItemKind::Assembly,
                latest: s.latest.as_ref().map(|v| v.version.to_string()),
                dirty: !s.changes.is_empty(),
                model: self.app.model_file(s.item.name.as_str()).ok().flatten(),
                stamp: self.app.stamp(&s.item),
                folder: folder_of(root, &s.item.dir),
            })
            .collect())
    }

    fn folders(&self) -> Result<Vec<String>> {
        let _files = self.reading();
        Ok(self.app.map()?.folders)
    }

    fn versions(&self, part: &str) -> Result<Vec<VersionEntry>> {
        let _files = self.reading();
        let mut list: Vec<VersionEntry> = self
            .app
            .versions(part)?
            .iter()
            .map(|v| VersionEntry {
                number: v.version.get(),
                date: short_date(&v.date),
                author: v.signature(),
                summary: v.summary().to_string(),
                hidden: v.hidden,
            })
            .collect();
        list.sort_by_key(|v| std::cmp::Reverse(v.number));
        Ok(list)
    }

    fn compare(
        &self,
        part: &str,
        before: Pick,
        after: Pick,
        progress: &dyn Progress,
    ) -> Result<Comparison> {
        self.compare_pair(part, before, after, progress)
    }

    fn changed_files(&self, part: &str) -> Result<Vec<String>> {
        let _files = self.reading();
        let item = self.app.item(part)?;
        Ok(self
            .app
            .changes(&item)?
            .into_iter()
            .map(|f| {
                let mark = match f.kind {
                    FileChangeKind::Added => "+",
                    FileChangeKind::Modified => "~",
                    FileChangeKind::Deleted => "−",
                };
                format!("{mark} {}", f.path)
            })
            .collect())
    }

    fn save_version(&self, part: &str, message: &str) -> Result<u32> {
        let _files = self.writing();
        Ok(self
            .app
            .save_version(part, message)
            .map_err(in_words)?
            .version
            .get())
    }

    fn mass(&self, part: &str, pick: Pick) -> Result<MassEntry> {
        let _files = self.reading();
        Ok(mass_entry(self.app.passport(part, version(pick)?)?))
    }

    fn materials(&self) -> Vec<MaterialEntry> {
        MATERIALS
            .iter()
            .map(|m| MaterialEntry {
                name: m.name.to_string(),
                density_kg_m3: m.density_kg_m3,
            })
            .collect()
    }

    fn set_material(
        &self,
        part: &str,
        material: Option<&str>,
        density_kg_m3: Option<f64>,
    ) -> Result<MassEntry> {
        let choice = material.map(|name| MaterialChoice {
            name: name.to_string(),
            density_kg_m3,
        });
        let _files = self.writing();
        Ok(mass_entry(
            self.app.set_material(part, choice).map_err(in_words)?,
        ))
    }

    fn files(&self, part: &str, before: Pick, after: Pick) -> Result<Vec<FileEntry>> {
        self.file_list(part, before, after)
    }

    fn file_diff(
        &self,
        part: &str,
        before: Pick,
        after: Pick,
        path: &str,
    ) -> Result<Vec<DiffEntry>> {
        self.file_lines(part, before, after, path)
    }

    fn open_file(&self, part: &str, pick: Pick, path: &str, way: &OpenWay) -> Result<PathBuf> {
        self.open_with(part, pick, path, way)
    }

    fn programs(&self, path: &str) -> Vec<ProgramEntry> {
        files::programs(path)
    }

    fn new_part(&self, name: &str, assembly: bool, folder: &str) -> Result<()> {
        let kind = if assembly {
            ItemKind::Assembly
        } else {
            ItemKind::Part
        };
        let _files = self.writing();
        self.app
            .new_item_in(name, kind, Path::new(folder))
            .map_err(in_words)?;
        Ok(())
    }

    fn new_folder(&self, folder: &str) -> Result<()> {
        let _files = self.writing();
        self.app.new_folder(Path::new(folder)).map_err(in_words)?;
        Ok(())
    }

    fn move_part(&self, part: &str, folder: &str) -> Result<()> {
        let _files = self.writing();
        self.app
            .move_item(part, Path::new(folder))
            .map_err(in_words)?;
        Ok(())
    }

    fn add_model(&self, file: &Path, part: &str, message: &str) -> Result<AddedModel> {
        // Импорт читает STEP, а OpenCascade работает по одному.
        let _one_at_a_time = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        let _files = self.writing();
        let imported = self.app.import(file, Some(part)).map_err(in_words)?;
        let name = imported.item.name.to_string();
        let version = match self.app.save_version(&name, message) {
            Ok(v) => Some(v.version.get()),
            Err(nonius_app::Error::Store(nonius_app::StoreError::NoChanges(_))) => None,
            Err(e) => return Err(in_words(e)),
        };
        Ok(AddedModel {
            part: name,
            version,
            warnings: imported.warnings,
        })
    }

    fn rename_part(&self, part: &str, new: &str) -> Result<()> {
        let _files = self.writing();
        self.app.rename_item(part, new).map_err(in_words)?;
        Ok(())
    }

    fn delete_part(&self, part: &str) -> Result<()> {
        let _files = self.writing();
        self.app.delete_item(part).map_err(in_words)
    }

    fn deleted_parts(&self) -> Result<Vec<DeletedEntry>> {
        let _files = self.reading();
        Ok(self
            .app
            .deleted_items()?
            .into_iter()
            .map(|d| DeletedEntry {
                name: d.name.to_string(),
                assembly: d.kind == ItemKind::Assembly,
                date: short_date(&d.date),
                author: d.author,
            })
            .collect())
    }

    fn recover_part(&self, part: &str) -> Result<()> {
        let _files = self.writing();
        self.app.recover_item(part).map_err(in_words)?;
        Ok(())
    }

    fn restore_version(&self, part: &str, version: u32) -> Result<u32> {
        let number = VersionNumber::new(version)?;
        let _files = self.writing();
        let done = self.app.restore(part, number, None).map_err(in_words)?;
        Ok(done.version.get())
    }

    fn add_file(&self, part: &str, file: &Path) -> Result<String> {
        let _files = self.writing();
        self.app.add_file(part, file).map_err(in_words)
    }

    fn remove_file(&self, part: &str, path: &str) -> Result<()> {
        let _files = self.writing();
        self.app.remove_file(part, path).map_err(in_words)
    }

    fn set_hidden(&self, part: &str, version: u32, hidden: bool) -> Result<()> {
        let number = VersionNumber::new(version)?;
        let _files = self.writing();
        self.app
            .set_hidden(part, number, hidden)
            .map_err(in_words)?;
        Ok(())
    }

    fn export_version(&self, part: &str, version: u32, dest: &Path) -> Result<usize> {
        let number = VersionNumber::new(version)?;
        let _files = self.reading();
        Ok(self.app.export(part, number, dest)?.1.len())
    }

    fn compare_outside(
        &self,
        part: &str,
        pick: Pick,
        other: &Other,
        progress: &dyn Progress,
    ) -> Result<Comparison> {
        self.compare_with(part, pick, other, progress)
    }

    fn old_layout(&self) -> bool {
        self.app.is_old_layout()
    }

    fn upgrade(&self) -> Result<()> {
        let _files = self.writing();
        self.app.upgrade().map_err(in_words)
    }
}

#[cfg(test)]
mod tests;
