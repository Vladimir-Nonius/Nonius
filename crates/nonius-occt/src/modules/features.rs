//! Сравнение словами инженера: какие отверстия, скругления и плоскости
//! изменились. Булевых операций не нужно — отчёт готов за доли секунды.

use nonius_core::compare::{ChangeReport, CompareContext};
use nonius_core::extension::{Comparator, ExtensionPoint, Module, ModuleManifest, Snapshot};
use nonius_core::Result;

use super::{manifest, model_pair, module_err, same_bytes};
use crate::changes;
use crate::shape::Shape;

/// Что сделали с деталью: «отверстие ⌀8 → ⌀10», «плоскость сдвинута».
pub struct FeatureComparator {
    manifest: ModuleManifest,
}

impl FeatureComparator {
    /// Идентификатор модуля.
    pub const ID: &'static str = "features-occt";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: manifest(
                Self::ID,
                "Что изменилось: отверстия, скругления, плоскости",
                ExtensionPoint::Comparator,
            ),
        }
    }
}

impl Default for FeatureComparator {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for FeatureComparator {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl Comparator for FeatureComparator {
    fn compare(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        ctx: &CompareContext,
    ) -> Result<ChangeReport> {
        let mut report = ChangeReport::empty(Self::ID);
        let Some((a, b)) = model_pair(before, after) else {
            return Ok(report);
        };
        if same_bytes(&a, &b) {
            return Ok(report);
        }
        let was = Shape::read_step(&a).map_err(|e| module_err(Self::ID, e))?;
        let now = Shape::read_step(&b).map_err(|e| module_err(Self::ID, e))?;
        // Версии не сопоставить — слов нет, объёмы скажет сравнение формы.
        if let Some(changed) = was.changed_faces(&now) {
            report.changes = changes::describe(&changed.faces, ctx.tolerance_mm);
        }
        Ok(report)
    }
}
