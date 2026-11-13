//! Откуда берётся форма деталей для проверки догадок (`shape.rs`).
//!
//! Форма — это объём, площадь, габариты, число граней. Откуда их взять, по
//! порядку:
//!
//! 1. Из самого файла CAD — «беглый взгляд» (точка `ShapeGlance`, модуль
//!    `nonius-sldprt` читает сетку показа из `.sldprt`). Доли секунды,
//!    чужая программа не нужна; числа чуть грубее (`Shape::rough`).
//! 2. Из STEP рядом, если он не старше файла CAD, — тем же модулем
//!    геометрии, что и при импорте.
//! 3. Через модуль перевода: STEP просим у самого SolidWorks (секунды на
//!    деталь, а если он завис — минуты). Сюда доходят только файлы, в
//!    которых сетки нет или она устарела.
//!
//! Беглый взгляд идёт первым и тогда, когда рядом лежит STEP: так у всех
//! версий одной детали числа из одного источника, и «копия ли это» видно
//! точно. Смотрим не всё подряд, а только то, что просили ([`ShapeCheck`]).
//!
//! Снятая форма запоминается (`memory.rs`): второй разбор той же папки
//! файлы заново не читает.
//!
//! Ничего не вышло (нет модуля, SolidWorks не ответил) — не беда: у детали
//! просто не будет отпечатка, и догадка останется непроверенной.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use nonius_core::meta::ItemKind;

use super::memory::Memory;
use super::shape::Shape;
use super::{key, IntakePlan, ShapeCheck};
use crate::{App, Progress};

pub(super) fn is_step(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| e == "step" || e == "stp")
}

pub(super) fn modified(path: &Path) -> SystemTime {
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
        let mut memory = Memory::load(&crate::work_dir(self.project.root()));
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
            let known = memory.get(models);
            if known.is_none() {
                progress.step(&format!(
                    "Смотрю форму {} из {}: {label}",
                    n + 1,
                    wanted.len()
                ));
            }
            let seen = known.or_else(|| {
                let shape = self.shape_of(models)?;
                memory.put(models, shape);
                Some(shape)
            });
            if let Some(shape) = seen {
                if std::env::var_os("NONIUS_TRACE").is_some() {
                    eprintln!("форма «{label}»: {shape:?}");
                }
                for model in *models {
                    shapes.insert(key(model), shape);
                }
            }
        }
        memory.save();
        shapes
    }

    /// Беглый взгляд в сам файл CAD. `None` — некому смотреть или в файле
    /// нет годных данных; причина видна с `NONIUS_TRACE`.
    fn glance(&self, cad: &Path) -> Option<Shape> {
        match self.registry.glance_for(cad)?.glance(cad) {
            Ok(summary) => Shape::of(&summary).map(|shape| Shape {
                rough: true,
                ..shape
            }),
            Err(e) => {
                if std::env::var_os("NONIUS_TRACE").is_some() {
                    eprintln!("беглый взгляд на {}: {e}", cad.display());
                }
                None
            }
        }
    }

    /// Отпечаток одной версии: беглый взгляд в файл CAD; не вышло — готовый
    /// STEP, если он не старше файла CAD рядом; иначе STEP просим у модуля
    /// перевода.
    fn shape_of(&self, models: &[PathBuf]) -> Option<Shape> {
        let step = models.iter().find(|m| is_step(m));
        let cad = models.iter().find(|m| !is_step(m));
        if let Some(shape) = cad.and_then(|c| self.glance(c)) {
            return Some(shape);
        }
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
