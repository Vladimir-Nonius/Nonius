//! Откуда берётся форма деталей для проверки догадок (`shape.rs`).
//!
//! Форма — это паспорт модели: объём, площадь, габариты, число граней. Его
//! читает тот же модуль геометрии, что и при импорте. У STEP паспорт
//! читается сразу; у файла SolidWorks сначала нужен STEP — его даёт модуль
//! перевода (сам SolidWorks, секунды на деталь). Поэтому смотрим не всё
//! подряд, а только то, что просили ([`ShapeCheck`]).
//!
//! Ничего не вышло (нет модуля, SolidWorks не ответил) — не беда: у детали
//! просто не будет отпечатка, и догадка останется непроверенной.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use nonius_core::meta::ItemKind;

use super::shape::Shape;
use super::{key, IntakePlan, ShapeCheck};
use crate::{App, Progress};

fn is_step(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| e == "step" || e == "stp")
}

fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

impl App {
    /// Снять отпечатки формы у моделей чернового плана `draft`. Ответ — по
    /// ключу каждого файла модели (см. [`key`]).
    pub(super) fn shapes_of(
        &self,
        draft: &IntakePlan,
        check: ShapeCheck,
        progress: &dyn Progress,
    ) -> HashMap<String, Shape> {
        let mut shapes = HashMap::new();
        // Сборки не смотрим: перевод большой сборки в STEP — это минуты.
        let wanted: Vec<(&str, &[PathBuf])> = draft
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Part)
            .filter(|i| match check {
                ShapeCheck::Off => false,
                ShapeCheck::Names => i.guessed,
                ShapeCheck::All => true,
            })
            .flat_map(|i| i.versions.iter())
            .map(|v| (v.label.as_str(), v.models.as_slice()))
            .collect();
        for (n, (label, models)) in wanted.iter().enumerate() {
            if progress.cancelled() {
                break;
            }
            progress.step(&format!(
                "Смотрю форму {} из {}: {label}",
                n + 1,
                wanted.len()
            ));
            if let Some(shape) = self.shape_of(models) {
                if std::env::var_os("NONIUS_TRACE").is_some() {
                    eprintln!("форма «{label}»: {shape:?}");
                }
                for model in *models {
                    shapes.insert(key(model), shape);
                }
            }
        }
        shapes
    }

    /// Отпечаток одной версии. Готовый STEP годится, если он не старше
    /// файла SolidWorks рядом; иначе STEP просим у модуля перевода.
    fn shape_of(&self, models: &[PathBuf]) -> Option<Shape> {
        let step = models.iter().find(|m| is_step(m));
        let cad = models.iter().find(|m| !is_step(m));
        let mut ignored = Vec::new();
        let read = |file: &Path, ignored: &mut Vec<String>| {
            let meta = self.read_geometry(file, ignored).ok().flatten()?;
            Shape::of(&meta.geometry)
        };
        let fresh = match (step, cad) {
            (Some(s), Some(c)) => modified(s) >= modified(c),
            (Some(_), None) => true,
            _ => false,
        };
        if fresh {
            return read(step?, &mut ignored);
        }
        let converted = cad
            .and_then(|c| self.convert(c, &mut ignored))
            .and_then(|c| c.read)
            .and_then(|meta| Shape::of(&meta.geometry));
        // SolidWorks не помог — лучше старый STEP, чем ничего.
        converted.or_else(|| read(step?, &mut ignored))
    }
}
