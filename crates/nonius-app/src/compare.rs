//! Сравнение двух версий детали: отчёты модулей и файлы отрисовщиков
//! (картинка `changes.png`, сцена `changes.glb`), с кэшем.
//!
//! Отчёты и файлы кэшируются отдельно: сравнение без картинок быстрое, а
//! картинки дорисовываются, когда их попросят.

use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::compare::{ChangeReport, CompareContext};
use nonius_core::extension::{ExtensionPoint, Snapshot};
use nonius_core::project::Item;
use nonius_core::schema::Versioned;
use nonius_core::{PartName, VersionNumber};
use nonius_store::{FileChange, VersionInfo};

use crate::cache::{self, CacheEntry};
use crate::{App, Error, Result};

/// Что посчитать при сравнении.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompareOptions {
    /// Нарисовать изменения (картинки, 3D-сцена). Дольше, чем одни числа.
    pub render: bool,
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

/// Одна сторона сравнения: снимок для модулей и отпечаток для ключа кэша.
struct Side {
    snapshot: Snapshot,
    id: String,
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
        let check = || match progress.cancelled() {
            true => Err(Error::Cancelled),
            false => Ok(()),
        };
        let item = self.item(name)?;
        let store = self.store()?;
        let before_info = match from {
            Some(v) => store.version(&item, v)?,
            None => store
                .latest(&item)?
                .ok_or_else(|| Error::NoVersions(item.name.clone()))?,
        };
        let after_info = to.map(|v| store.version(&item, v)).transpose()?;

        progress.step("Готовлю версии");
        let before = self.side(&item, Some(&before_info))?;
        let after = self.side(&item, after_info.as_ref())?;
        check()?;

        let ctx = CompareContext {
            tolerance_mm: self.project.config().compare.tolerance_mm,
            ..CompareContext::default()
        };
        let mut out = Comparison {
            item: item.name.clone(),
            from: before_info.version,
            to: after_info.as_ref().map(|v| v.version),
            reports: Vec::new(),
            file_changes: Vec::new(),
            files: Vec::new(),
            from_cache: true,
        };
        let labels = (out.from_label(), out.to_label());

        let (entry, _, cached) =
            self.cached("reports", &before, &after, &ctx, &labels, |_, base| {
                progress.step("Сравниваю");
                let reports = self
                    .registry
                    .compare_all(&before.snapshot, &after.snapshot, &ctx)?;
                check()?;
                Ok(CacheEntry { reports, ..base })
            })?;
        out.reports = entry.reports;
        out.from_cache &= cached;

        if options.render {
            if self.modules_of(ExtensionPoint::Renderer).is_empty() {
                return Err(Error::NoRenderer);
            }
            let merged = ChangeReport {
                comparator: "nonius-app".into(),
                changes: out.reports.iter().flat_map(|r| r.changes.clone()).collect(),
            };
            let (entry, dir, cached) =
                self.cached("files", &before, &after, &ctx, &labels, |dir, base| {
                    let mut files = Vec::new();
                    for r in self.registry.renderers() {
                        check()?;
                        progress.step("Рисую изменения");
                        let id = r.manifest().id.clone();
                        let rel_dir = format!("files/{id}");
                        let made = r.render(
                            &before.snapshot,
                            &after.snapshot,
                            &merged,
                            &dir.join(&rel_dir),
                        )?;
                        for f in made {
                            files.push(keep_file(dir, &rel_dir, &f)?);
                        }
                    }
                    check()?;
                    Ok(CacheEntry { files, ..base })
                })?;
            out.files = entry.files.iter().map(|f| dir.join(f)).collect();
            out.from_cache &= cached;
        }

        if to.is_none() {
            out.file_changes = store.changes(&item)?;
        }
        Ok(out)
    }

    fn side(&self, item: &Item, version: Option<&VersionInfo>) -> Result<Side> {
        let store = self.store()?;
        Ok(match version {
            Some(v) => Side {
                snapshot: store.snapshot(item, v)?,
                id: format!("версия {}", v.commit),
            },
            None => Side {
                id: format!("текущее {}", cache::dir_fingerprint(&item.dir)?),
                snapshot: store.working_snapshot(item)?,
            },
        })
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

    /// Взять запись из кэша или посчитать её `compute` и сохранить.
    /// Возвращает запись, её папку и признак «из кэша».
    fn cached(
        &self,
        kind: &str,
        before: &Side,
        after: &Side,
        ctx: &CompareContext,
        labels: &(String, String),
        compute: impl FnOnce(&Path, CacheEntry) -> Result<CacheEntry>,
    ) -> Result<(CacheEntry, PathBuf, bool)> {
        // Картинки зависят и от отчётов: в ключе файлов — оба набора модулей.
        let mut modules = self.modules_of(ExtensionPoint::Comparator);
        if kind == "files" {
            modules.extend(self.modules_of(ExtensionPoint::Renderer));
        }
        let mut parts = vec![
            format!("nonius-cache {}", CacheEntry::CURRENT),
            kind.to_string(),
            before.id.clone(),
            after.id.clone(),
            format!("{} {}", ctx.tolerance_mm, ctx.relative_tolerance),
        ];
        parts.extend(modules.iter().cloned());
        let dir = self.cache_dir().join(cache::key(&parts));
        if let Some(entry) = cache::load(&dir) {
            return Ok((entry, dir, true));
        }
        let base = CacheEntry {
            schema: CacheEntry::CURRENT,
            item: before.snapshot.item.to_string(),
            from: labels.0.clone(),
            to: labels.1.clone(),
            modules,
            reports: Vec::new(),
            files: Vec::new(),
        };
        let entry = cache::store(&dir, |tmp| compute(tmp, base))?;
        Ok((entry, dir, false))
    }

    pub(crate) fn cache_dir(&self) -> PathBuf {
        self.work_dir().join("cache")
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
