//! Модули Nonius на OpenCascade: чтение STEP с контрольными числами,
//! сравнение геометрии и цветная подсветка изменений.
//!
//! Файлы: здесь — общие помощники и превью, `source.rs` — чтение STEP,
//! `diff.rs` — разница двух тел, `compare.rs` — сравнение формы,
//! `highlight.rs` — картинка изменений.

mod compare;
mod diff;
mod highlight;
mod source;

use std::path::{Path, PathBuf};

use nonius_core::extension::{ExtensionPoint, ModuleManifest, Snapshot, API_VERSION};
use nonius_core::Error;

use crate::render::{colors, render, Image, Layer, LayerKind, View};
use crate::shape::Shape;
use crate::OcctError;

pub use compare::GeometryComparator;
pub use diff::GeometryDiff;
pub use highlight::HighlightRenderer;
pub use source::OcctStepSource;

/// Имя модели в папке детали.
pub const MODEL_STEP: &str = "model.step";
/// Имя картинки изменений, которую рисует [`HighlightRenderer`].
pub const CHANGES_PNG: &str = "changes.png";

fn manifest(id: &str, name: &str, point: ExtensionPoint) -> ModuleManifest {
    ModuleManifest {
        id: id.into(),
        name: name.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        api: API_VERSION,
        points: vec![point],
        permissions: vec![],
    }
}

fn module_err(id: &str, e: OcctError) -> Error {
    Error::Module {
        module: id.into(),
        message: e.0,
    }
}

fn is_step(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("step") || e.eq_ignore_ascii_case("stp"))
}

/// Допуск сетки по размеру тела: мелкие детали — мельче.
fn deflection(props_bbox: Option<([f64; 3], [f64; 3])>) -> f64 {
    let diag = props_bbox
        .map(|(lo, hi)| {
            ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt()
        })
        .unwrap_or(100.0);
    (diag * 1.5e-3).max(0.005)
}

const ANGULAR: f64 = 0.3;

/// Нарисовать превью одной детали из STEP.
pub fn preview(step: &Path, view: &View) -> std::result::Result<Image, OcctError> {
    let shape = Shape::read_step(step)?;
    let mesh = shape.mesh(deflection(shape.rough_bbox()), ANGULAR);
    Ok(render(
        &[Layer {
            mesh: &mesh,
            color: colors::PREVIEW,
            kind: LayerKind::Context,
        }],
        view,
    ))
}

fn model_pair(before: &Snapshot, after: &Snapshot) -> Option<(PathBuf, PathBuf)> {
    let a = before.dir.join(MODEL_STEP);
    let b = after.dir.join(MODEL_STEP);
    (a.is_file() && b.is_file()).then_some((a, b))
}

fn same_bytes(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) if ma.len() != mb.len() => false,
        _ => matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y),
    }
}
