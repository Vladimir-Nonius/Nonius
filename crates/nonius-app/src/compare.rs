//! Сравнение двух версий детали: отчёты модулей и файлы отрисовщиков
//! (картинка `changes.png`, сцена `changes.glb`), с кэшем.
//!
//! Отчёты и файлы кэшируются отдельно: сравнение без картинок быстрое, а
//! картинки дорисовываются, когда их попросят.

use std::cell::OnceCell;
use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::compare::{ChangeReport, CompareContext};
use nonius_core::extension::{ExtensionPoint, Snapshot};
use nonius_core::project::Item;
use nonius_core::schema::Versioned;
use nonius_core::{PartName, VersionNumber};
use nonius_store::{workdir, FileChange, StoreError, VersionInfo};

use crate::cache::{self, CacheEntry};
use crate::{App, Error, Result};

/// Что посчитать при сравнении. По умолчанию — только отчёты (быстро).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompareOptions {
    /// Отчёты модулей сравнения: числа, материал, форма.
    pub reports: bool,
    /// Нарисовать изменения (картинки, 3D-сцена). Дольше, чем одни числа.
    pub render: bool,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            reports: true,
            render: false,
        }
    }
}

/// Ход долгого действия и его отмена. Окно показывает шаги и даёт кнопку
/// «Отмена»; командная строка передаёт [`NoProgress`].
pub trait Progress: Sync {
    /// Начался шаг: «Сравниваю», «Рисую изменения».
    fn step(&self, what: &str) {
        let _ = what;
    }
    /// Пользователь попросил отменить. Проверяется между шагами;
    /// отменённый расчёт в кэш не попадает.
    fn cancelled(&self) -> bool {
        false
    }
}

/// Без хода выполнения и без отмены.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProgress;

impl Progress for NoProgress {}

/// Итог сравнения.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// Деталь.
    pub item: PartName,
    /// «Было».
    pub from: VersionNumber,
    /// «Стало»; `None` — текущее состояние.
    pub to: Option<VersionNumber>,
    /// Отчёты модулей сравнения.
    pub reports: Vec<ChangeReport>,
    /// Изменённые файлы детали (только когда «стало» — текущее состояние).
    pub file_changes: Vec<FileChange>,
    /// Файлы отрисовщиков (если просили `render`). Лежат в кэше: не меняйте
    /// их, а копируйте.
    pub files: Vec<PathBuf>,
    /// Всё взято из кэша, ничего не считалось.
    pub from_cache: bool,
}

impl Comparison {
    /// Подпись «было»: `v3`.
    pub fn from_label(&self) -> String {
        self.from.to_string()
    }

    /// Подпись «стало»: `v4` или `текущее`.
    pub fn to_label(&self) -> String {
        self.to.map_or_else(|| "текущее".into(), |v| v.to_string())
    }

    /// Файл отрисовщика по имени: `changes.png`, `changes.glb`.
    pub fn file(&self, name: &str) -> Option<&Path> {
        self.files
            .iter()
            .find(|p| p.file_name().is_some_and(|n| n == name))
            .map(PathBuf::as_path)
    }

    /// Изменений нет ни в отчётах, ни в файлах.
    pub fn is_empty(&self) -> bool {
        self.reports.iter().all(ChangeReport::is_empty) && self.file_changes.is_empty()
    }
}

/// Снимки обеих версий для модулей. Готовятся только при промахе кэша:
/// выложить версию из git дороже, чем найти готовый результат.
struct Sides<'a> {
    app: &'a App,
    item: &'a Item,
    before: &'a VersionInfo,
    after: Option<&'a VersionInfo>,
    snapshots: OnceCell<(Snapshot, Snapshot)>,
}

impl Sides<'_> {
    fn get(&self) -> Result<&(Snapshot, Snapshot)> {
        if let Some(s) = self.snapshots.get() {
            return Ok(s);
        }
        let store = self.app.store()?;
        let before = store.snapshot(self.item, self.before)?;
        let after = match self.after {
            Some(v) => store.snapshot(self.item, v)?,
            None => store.working_snapshot(self.item)?,
        };
        Ok(self.snapshots.get_or_init(|| (before, after)))
    }
}

impl App {
    /// Сравнить версии детали: `from` (по умолчанию последняя) и `to`
    /// (по умолчанию текущее состояние).
    pub fn compare(
        &self,
        name: &str,
        from: Option<VersionNumber>,
        to: Option<VersionNumber>,
        options: &CompareOptions,
        progress: &dyn Progress,
    ) -> Result<Comparison> {
        let check = || {
            if progress.cancelled() {
                Err(Error::Cancelled)
            } else {
                Ok(())
            }
        };
        let item = self.item(name)?;
        let store = self.store()?;
        let versions = store.versions(&item)?;
        let pick = |n: VersionNumber| {
            versions.iter().find(|v| v.version == n).ok_or_else(|| {
                Error::Store(StoreError::UnknownVersion {
                    item: item.name.clone(),
                    version: n,
                })
            })
        };
        let before = match from {
            Some(n) => pick(n)?,
            None => versions
                .first()
                .ok_or_else(|| Error::NoVersions(item.name.clone()))?,
        };
        let after = to.map(pick).transpose()?;

        progress.step("Готовлю версии");
        let ctx = CompareContext {
            tolerance_mm: self.project.config().compare.tolerance_mm,
            ..CompareContext::default()
        };
        let after_id = match after {
            Some(v) => format!("версия {}", v.commit),
            None => format!("текущее {}", cache::dir_fingerprint(&item.dir)?),
        };
        let key = vec![
            format!("nonius-cache {}", CacheEntry::CURRENT),
            format!("версия {}", before.commit),
            after_id,
            format!("{} {}", ctx.tolerance_mm, ctx.relative_tolerance),
        ];
        let sides = Sides {
            app: self,
            item: &item,
            before,
            after,
            snapshots: OnceCell::new(),
        };
        check()?;

        let mut out = Comparison {
            item: item.name.clone(),
            from: before.version,
            to: after.map(|v| v.version),
            reports: Vec::new(),
            file_changes: Vec::new(),
            files: Vec::new(),
            from_cache: true,
        };
        let base = CacheEntry {
            schema: CacheEntry::CURRENT,
            item: item.name.to_string(),
            from: out.from_label(),
            to: out.to_label(),
            modules: Vec::new(),
            reports: Vec::new(),
            files: Vec::new(),
        };
        let comparators = if options.reports {
            self.modules_of(ExtensionPoint::Comparator)
        } else {
            Vec::new()
        };

        if options.reports {
            let (entry, _, cached) =
                self.cached("reports", &key, comparators.clone(), &base, |_| {
                    let (a, b) = sides.get()?;
                    progress.step("Сравниваю");
                    let reports = self.registry.compare_all(a, b, &ctx)?;
                    check()?;
                    Ok((reports, Vec::new()))
                })?;
            out.reports = entry.reports;
            out.from_cache &= cached;
        }

        if options.render {
            if self.registry.renderers().next().is_none() {
                return Err(Error::NoRenderer);
            }
            // Картинки зависят и от отчётов: в ключе — оба набора модулей.
            let mut modules = comparators;
            modules.extend(self.modules_of(ExtensionPoint::Renderer));
            let (entry, dir, cached) = self.cached("files", &key, modules, &base, |dir| {
                let (a, b) = sides.get()?;
                let merged = ChangeReport {
                    comparator: "nonius-app".into(),
                    changes: out.reports.iter().flat_map(|r| r.changes.clone()).collect(),
                };
                let mut files = Vec::new();
                for r in self.registry.renderers() {
                    check()?;
                    progress.step("Рисую изменения");
                    let rel_dir = format!("files/{}", r.manifest().id);
                    for f in r.render(a, b, &merged, &dir.join(&rel_dir))? {
                        files.push(keep_file(dir, &rel_dir, &f)?);
                    }
                }
                check()?;
                Ok((Vec::new(), files))
            })?;
            out.files = entry.files.iter().map(|f| dir.join(f)).collect();
            out.from_cache &= cached;
        }

        if !out.from_cache {
            cache::prune(&self.cache_dir(), cache::KEEP_ENTRIES);
        }
        if to.is_none() {
            out.file_changes = store.changes(&item)?;
        }
        Ok(out)
    }

    /// `id версия` модулей точки — часть ключа кэша.
    fn modules_of(&self, point: ExtensionPoint) -> Vec<String> {
        self.registry
            .manifests()
            .into_iter()
            .filter(|m| m.points.contains(&point))
            .map(|m| format!("{} {}", m.id, m.version))
            .collect()
    }

    /// Взять запись из кэша или посчитать `compute` (отчёты, файлы) и
    /// сохранить. Возвращает запись, её папку и признак «из кэша».
    fn cached(
        &self,
        kind: &str,
        key: &[String],
        modules: Vec<String>,
        base: &CacheEntry,
        compute: impl FnOnce(&Path) -> Result<(Vec<ChangeReport>, Vec<String>)>,
    ) -> Result<(CacheEntry, PathBuf, bool)> {
        let mut parts = key.to_vec();
        parts.push(kind.to_string());
        parts.extend(modules.iter().cloned());
        let dir = self.cache_dir().join(cache::key(&parts));
        if let Some(entry) = cache::load(&dir) {
            return Ok((entry, dir, true));
        }
        let entry = cache::store(&dir, |tmp| {
            let (reports, files) = compute(tmp)?;
            Ok(CacheEntry {
                modules,
                reports,
                files,
                ..base.clone()
            })
        })?;
        Ok((entry, dir, false))
    }

    fn cache_dir(&self) -> PathBuf {
        workdir::work_dir(self.project.root()).join("cache")
    }
}

/// Файл отрисовщика — путь внутри записи. Если модуль положил файл в другое
/// место, он копируется в запись.
fn keep_file(entry_dir: &Path, rel_dir: &str, file: &Path) -> Result<String> {
    if let Ok(rel) = file.strip_prefix(entry_dir) {
        return Ok(rel.to_string_lossy().replace('\\', "/"));
    }
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    let rel = format!("{rel_dir}/{name}");
    let dest = entry_dir.join(&rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    fs::copy(file, &dest).map_err(|e| Error::io(&dest, e))?;
    Ok(rel)
}
