//! Сравнение детали с тем, чего нет в её истории: с файлом на диске или с
//! другой деталью. Ничего не сохраняется: копия файла лежит в служебной папке,
//! пока не понадобится другой файл.

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
                let reports = self.registry.compare_all(&before, &after, &ctx)?;
                cancelled().map_or(Ok((reports, Vec::new())), Err)
            })?;
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
                let files = self.render_all(&before, &after, &merged, dir, progress)?;
                cancelled().map_or(Ok((Vec::new(), files)), Err)
            })?;
            out.files = entry.files.iter().map(|f| dir.join(f)).collect();
        }
        Ok(out)
    }

    /// Файл на диске как снимок детали и его отпечаток для ключа кэша.
    fn file_snapshot(&self, path: &Path) -> Result<(Snapshot, String)> {
        if !path.is_file() {
            return Err(Error::FileNotFound(path.to_path_buf()));
        }
        let extension = path.extension().and_then(|e| e.to_str());
        let step = extension.map(str::to_ascii_lowercase);
        if !matches!(step.as_deref(), Some("step" | "stp")) {
            return Err(Error::NotStep(path.to_path_buf()));
        }
        let mut warnings = Vec::new();
        let meta = self.read_geometry(path, &mut warnings)?;
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
        let name = path.file_stem().unwrap_or_default().to_string_lossy();
        let snapshot = Snapshot {
            item: PartName::cleaned(&name).unwrap_or_else(|| PartName::new("файл").expect("имя")),
            version: None,
            dir,
            meta: meta.unwrap_or_else(|| PartMeta::new(ItemKind::Part)),
            tree: None,
        };
        Ok((snapshot, format!("файл {print}")))
    }
}
