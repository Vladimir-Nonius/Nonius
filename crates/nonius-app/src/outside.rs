//! Сравнение детали с тем, чего нет в её истории: с файлом на диске или с
//! другой деталью. Ничего не сохраняется: копия файла лежит в служебной папке,
//! пока не понадобится другой файл.
//!
//! И сравнение двух файлов, которых нет в проекте ([`App::compare_files`]):
//! так разбор папки показывает в 3D, чем отличаются две модели. Кэш
//! сравнений его не касается — копии и картинки живут во временной папке.

use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::compare::{ChangeReport, CompareContext};
use nonius_core::extension::{ExtensionPoint, Snapshot};
use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::schema::Versioned;
use nonius_core::{PartName, VersionNumber};
use nonius_store::workdir;
use sha2::{Digest, Sha256};

use crate::cache::{self, CacheEntry};
use crate::compare::{CompareOptions, Progress};
use crate::{App, Error, Result};

/// Папка с копией файла внутри `.git/nonius`.
const OUTSIDE_DIR: &str = "outside";
/// Имя копии: модули сравнения ищут модель под этим именем.
const MODEL_STEP: &str = "model.step";

/// С чем сравнить деталь.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Other<'a> {
    /// Файл STEP на диске.
    File(&'a Path),
    /// Другая деталь, как она есть сейчас.
    Part(&'a str),
}

/// Итог сравнения с файлом или деталью.
#[derive(Debug, Clone, PartialEq)]
pub struct Outside {
    /// Отчёты модулей сравнения.
    pub reports: Vec<ChangeReport>,
    /// Файлы отрисовщиков (если просили `render`).
    pub files: Vec<PathBuf>,
}

/// Итог сравнения двух файлов с диска. Файлы отрисовщиков лежат во
/// временной папке и живут, пока жив этот итог.
#[derive(Debug)]
pub struct FilesCompared {
    /// Отчёты модулей сравнения.
    pub reports: Vec<ChangeReport>,
    /// Файлы отрисовщиков разницы. Пусто — нечем рисовать или разница не
    /// посчиталась (тогда есть `problem` и `apart`).
    pub files: Vec<PathBuf>,
    /// Почему разницу нарисовать не вышло.
    pub problem: Option<String>,
    /// Разницы нет — файлы отрисовщиков «было» и «стало» по отдельности:
    /// показать две модели рядом.
    pub apart: Option<(Vec<PathBuf>, Vec<PathBuf>)>,
    /// STEP какой-то стороны старше её файла SolidWorks: форма может быть
    /// прежней (ставит [`App::intake_compare`]).
    pub step_behind: bool,
    _dir: tempfile::TempDir,
}

impl App {
    /// Сравнить деталь (версию `at` или текущее состояние) с `other`.
    pub fn compare_outside(
        &self,
        name: &str,
        at: Option<VersionNumber>,
        other: Other<'_>,
        options: &CompareOptions,
        progress: &dyn Progress,
    ) -> Result<Outside> {
        let item = self.item(name)?;
        let store = self.store()?;
        progress.step("Готовлю версии");
        let (before, side) = match at {
            Some(n) => {
                let v = store.version(&item, n)?;
                (store.snapshot(&item, &v)?, format!("версия {}", v.commit))
            }
            None => {
                let print = cache::model_fingerprint(&item.dir, true)?;
                (store.working_snapshot(&item)?, format!("текущее {print}"))
            }
        };
        let (after, other_side) = match other {
            Other::File(path) => self.file_snapshot(path)?,
            Other::Part(part) => {
                let o = self.item(part)?;
                let print = cache::model_fingerprint(&o.dir, true)?;
                (store.working_snapshot(&o)?, format!("деталь {print}"))
            }
        };
        let ctx = CompareContext {
            tolerance_mm: self.project.config().compare.tolerance_mm,
            ..CompareContext::default()
        };
        let key = vec![
            format!("nonius-cache {}", CacheEntry::CURRENT),
            side,
            other_side,
            format!("{} {}", ctx.tolerance_mm, ctx.relative_tolerance),
        ];
        let base = CacheEntry {
            schema: CacheEntry::CURRENT,
            item: item.name.to_string(),
            from: at.map_or_else(|| "текущее".into(), |v| v.to_string()),
            to: "вне истории".into(),
            modules: Vec::new(),
            reports: Vec::new(),
            files: Vec::new(),
        };
        let cancelled = || progress.cancelled().then_some(Error::Cancelled);
        let comparators = self.modules_of(ExtensionPoint::Comparator);
        let mut out = Outside {
            reports: Vec::new(),
            files: Vec::new(),
        };
        if options.reports {
            let modules = comparators.clone();
            let (entry, _, _) = self.cached("outside-reports", &key, modules, &base, |_| {
                progress.step("Сравниваю");
                // Досчитанное кладём в кэш и при отмене (см. `compare.rs`).
                let reports = self.registry.compare_all(&before, &after, &ctx)?;
                Ok((reports, Vec::new()))
            })?;
            cancelled().map_or(Ok(()), Err)?;
            out.reports = entry.reports;
        }
        if options.render {
            if self.registry.renderers().next().is_none() {
                return Err(Error::NoRenderer);
            }
            let mut modules = comparators;
            modules.extend(self.modules_of(ExtensionPoint::Renderer));
            let (entry, dir, _) = self.cached("outside-files", &key, modules, &base, |dir| {
                let merged = ChangeReport {
                    comparator: "nonius-app".into(),
                    changes: out.reports.iter().flat_map(|r| r.changes.clone()).collect(),
                };
                let files = self.render_all(&before, &after, &merged, &ctx, dir, progress)?;
                Ok((Vec::new(), files))
            })?;
            cancelled().map_or(Ok(()), Err)?;
            out.files = entry.files.iter().map(|f| dir.join(f)).collect();
        }
        Ok(out)
    }

    /// Сравнить два файла STEP с диска, которых нет в проекте (разбор
    /// папки). Не вышло нарисовать разницу — рисуется каждая сторона
    /// отдельно, а причина — в `problem`.
    pub fn compare_files(
        &self,
        before: &Path,
        after: &Path,
        progress: &dyn Progress,
    ) -> Result<FilesCompared> {
        let tmp = tempfile::tempdir().map_err(|e| Error::io(std::env::temp_dir(), e))?;
        let dir = tmp.path().to_path_buf();
        let copy = |path: &Path, side: &str| -> Result<Snapshot> {
            let meta = self.step_meta(path)?;
            let into = dir.join(side);
            fs::create_dir_all(&into).map_err(|e| Error::io(&into, e))?;
            let to = into.join(MODEL_STEP);
            fs::copy(path, &to).map_err(|e| Error::io(&to, e))?;
            Ok(snapshot_at(path, into, meta))
        };
        let (was, now) = (copy(before, "before")?, copy(after, "after")?);
        let ctx = CompareContext {
            tolerance_mm: self.project.config().compare.tolerance_mm,
            ..CompareContext::default()
        };
        progress.step("Сравниваю");
        // Слова и числа нужны и тогда, когда тяжёлое сравнение не сошлось.
        let reports = match self.registry.compare_all(&was, &now, &ctx) {
            Ok(reports) => reports,
            Err(_) => (self.registry.compare_quick(&was, &now, &ctx)).unwrap_or_default(),
        };
        if progress.cancelled() {
            return Err(Error::Cancelled);
        }
        let mut out = FilesCompared {
            reports,
            files: Vec::new(),
            problem: None,
            apart: None,
            step_behind: false,
            _dir: tmp,
        };
        if self.registry.renderers().next().is_none() {
            return Ok(out);
        }
        let draw = |a: &Snapshot, b: &Snapshot, report: &ChangeReport, sub: &str| {
            let at = dir.join(sub);
            let files = self.render_all(a, b, report, &ctx, &at, progress)?;
            Ok::<_, Error>(files.iter().map(|f| at.join(f)).collect::<Vec<_>>())
        };
        let merged = ChangeReport {
            comparator: "nonius-app".into(),
            changes: out.reports.iter().flat_map(|r| r.changes.clone()).collect(),
        };
        match draw(&was, &now, &merged, "drawn") {
            Ok(files) => out.files = files,
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(e) => {
                out.problem = Some(format!("{e:#}"));
                let empty = ChangeReport::empty("nonius-app");
                let before = draw(&was, &was, &empty, "before-alone")?;
                let after = draw(&now, &now, &empty, "after-alone")?;
                out.apart = Some((before, after));
            }
        }
        Ok(out)
    }

    /// Паспорт файла STEP с диска; нет файла или он не STEP — ошибка.
    fn step_meta(&self, path: &Path) -> Result<PartMeta> {
        if !path.is_file() {
            return Err(Error::FileNotFound(path.to_path_buf()));
        }
        let extension = path.extension().and_then(|e| e.to_str());
        let step = extension.map(str::to_ascii_lowercase);
        if !matches!(step.as_deref(), Some("step" | "stp")) {
            return Err(Error::NotStep(path.to_path_buf()));
        }
        let meta = self.read_geometry(path, &mut Vec::new())?;
        Ok(meta.unwrap_or_else(|| PartMeta::new(ItemKind::Part)))
    }

    /// Файл на диске как снимок детали и его отпечаток для ключа кэша.
    fn file_snapshot(&self, path: &Path) -> Result<(Snapshot, String)> {
        let meta = self.step_meta(path)?;
        let bytes = fs::read(path).map_err(|e| Error::io(path, e))?;
        let print: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let base = workdir::work_dir(self.project.root()).join(OUTSIDE_DIR);
        let dir = base.join(&print);
        let ready = |d: &Path| d.join(MODEL_STEP).is_file();
        if !ready(&dir) {
            let tmp = workdir::tmp_dir_for(&dir).map_err(|e| Error::io(&dir, e))?;
            fs::write(tmp.join(MODEL_STEP), &bytes).map_err(|e| Error::io(&tmp, e))?;
            workdir::publish_dir(&tmp, &dir, ready).map_err(|e| Error::io(&dir, e))?;
        }
        // Нужна только последняя копия: модели большие.
        for entry in fs::read_dir(&base).into_iter().flatten().flatten() {
            if entry.path() != dir {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
        Ok((snapshot_at(path, dir, meta), format!("файл {print}")))
    }
}

/// Снимок «детали» из файла `path`, чья копия `model.step` лежит в `dir`.
fn snapshot_at(path: &Path, dir: PathBuf, meta: PartMeta) -> Snapshot {
    let name = path.file_stem().unwrap_or_default().to_string_lossy();
    Snapshot {
        item: PartName::cleaned(&name).unwrap_or_else(|| PartName::new("файл").expect("имя")),
        version: None,
        dir,
        meta,
        tree: None,
    }
}
