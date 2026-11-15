//! STEP от модуля перевода (SolidWorks) для разбора папки: разрешение его
//! будить и точная форма по такому STEP.
//!
//! Сам STEP помнит склад на диске (`steps.rs`): форма и слова одной версии
//! просят его у SolidWorks один раз — и в этом проходе, и в следующем
//! разборе.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::key;
use super::memory::Memory;
use super::shape::{Shape, Source};
use crate::import::Converted;
use crate::steps::Stored;
use crate::App;

/// Разрешение будить модуль перевода: без него (`None` там, где его ждут)
/// модуль не зовётся вовсе. Здесь помнятся только неудачи «сейчас не вышло»
/// (SolidWorks не ответил): за один проход вторая попытка ждала бы столько
/// же. Удачи и окончательные отказы помнит склад.
#[derive(Default)]
pub(super) struct Exports(RefCell<HashSet<String>>);

impl Exports {
    /// STEP файла CAD `cad`; не вышло — почему, в `warnings` (только в
    /// первый раз).
    pub fn of(&self, app: &App, cad: &Path, warnings: &mut Vec<String>) -> Option<Converted> {
        if self.0.borrow().contains(&key(cad)) {
            return None;
        }
        let got = app.convert(cad, warnings);
        if got.is_none() {
            self.0.borrow_mut().insert(key(cad));
        }
        got
    }
}

impl App {
    /// Точная форма по STEP от модуля перевода. Формы не будет и в
    /// следующий раз — это запоминается (`Memory::fail`), чтобы версию не
    /// уточняли при каждом разборе заново:
    /// - STEP получен, а тела в нём нет (поверхности, пустая деталь) —
    ///   пометка остаётся и на складе STEP, для копий файла в других папках;
    /// - модуль отказался окончательно (деталь роняет SolidWorks) — отказ
    ///   лежит на складе.
    ///
    /// Не ответил SolidWorks сейчас — может, ответит потом: не запоминаем.
    pub(super) fn exported(
        &self,
        cad: &Path,
        exports: &Exports,
        models: &[PathBuf],
        memory: &mut Memory,
        ignored: &mut Vec<String>,
    ) -> Option<Shape> {
        let store = self.steps();
        let key = self.step_key(cad);
        if key.as_ref().is_some_and(|k| store.is_empty(k)) {
            memory.fail(models, "тела нет");
            return None;
        }
        let Some(converted) = exports.of(self, cad, ignored) else {
            let refused = key.as_ref().and_then(|k| store.get(k));
            if let Some(Stored::Refused(why)) = refused {
                memory.fail(models, &why);
            }
            return None;
        };
        let read = converted.read.as_ref();
        let got = read.and_then(|m| Shape::of(&m.geometry, Source::Exact));
        if got.is_none() {
            memory.fail(models, "тела нет");
            if let Some(key) = &converted.key {
                store.mark_empty(key);
            }
        }
        got
    }
}
