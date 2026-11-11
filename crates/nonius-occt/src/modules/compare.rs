//! Сравнение формы: сколько материала добавлено и убрано.

use nonius_core::compare::{Change, ChangeKind, ChangeReport, CompareContext};
use nonius_core::extension::{Comparator, ExtensionPoint, Module, ModuleManifest, Snapshot};
use nonius_core::Result;

use super::{manifest, model_pair, module_err, same_bytes, shared_diff};

/// Сравнение формы через булевы операции: сколько материала добавлено и убрано.
pub struct GeometryComparator {
    manifest: ModuleManifest,
}

impl GeometryComparator {
    /// Идентификатор модуля.
    pub const ID: &'static str = "geometry-occt";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: manifest(
                Self::ID,
                "Сравнение формы (OpenCascade)",
                ExtensionPoint::Comparator,
            ),
        }
    }
}

impl Default for GeometryComparator {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for GeometryComparator {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

fn volume_text(volume: f64, pieces: u32) -> String {
    let v = if volume >= 100.0 {
        format!("{volume:.0} мм³")
    } else {
        format!("{volume:.2} мм³")
    };
    if pieces > 1 {
        format!("{v}, мест: {pieces}")
    } else {
        v
    }
}

impl Comparator for GeometryComparator {
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
        let diff = shared_diff(&a, &b, ctx.tolerance_mm).map_err(|e| module_err(Self::ID, e))?;
        // Честные оговорки: строки без цвета «добавлено / убрано».
        let note = |subject: String| Change {
            kind: ChangeKind::Modified,
            subject,
            before: None,
            after: None,
        };
        if diff.no_solids {
            report.changes.push(note(
                "Форма не сравнена: в файле нет твёрдых тел, только поверхности".into(),
            ));
        }
        if let Some(off) = diff.unsure {
            report.changes.push(note(format!(
                "Сравнение формы приблизительное: объёмы не сходятся на {}",
                volume_text(off, 1)
            )));
        }
        if diff.has_added() {
            report.changes.push(Change {
                kind: ChangeKind::Added,
                subject: "Добавлен материал".into(),
                before: None,
                after: Some(volume_text(diff.added_volume, diff.added_pieces)),
            });
        }
        if diff.has_removed() {
            report.changes.push(Change {
                kind: ChangeKind::Removed,
                subject: "Убран материал".into(),
                before: Some(volume_text(diff.removed_volume, diff.removed_pieces)),
                after: None,
            });
        }
        Ok(report)
    }
}
