//! Источник геометрии: STEP через OpenCascade.

use std::path::Path;

use nonius_core::extension::{ExtensionPoint, GeometrySource, Module, ModuleManifest};
use nonius_core::meta::{BoundingBox, ItemKind, PartMeta};
use nonius_core::Result;
use nonius_step::StepHeaderSource;

use super::{is_step, manifest, module_err};
use crate::shape::Shape;

/// Чтение STEP через OpenCascade: объём, площадь, центр масс, габариты, число
/// тел и граней. Сведения заголовка (CAD, автор, протокол) — как у `step-header`.
pub struct OcctStepSource {
    manifest: ModuleManifest,
}

impl OcctStepSource {
    /// Идентификатор модуля.
    pub const ID: &'static str = "step-occt";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: manifest(
                Self::ID,
                "Геометрия STEP (OpenCascade)",
                ExtensionPoint::GeometrySource,
            ),
        }
    }
}

impl Default for OcctStepSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for OcctStepSource {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl GeometrySource for OcctStepSource {
    fn can_read(&self, path: &Path) -> bool {
        is_step(path)
    }

    fn read(&self, path: &Path) -> Result<PartMeta> {
        let mut meta = StepHeaderSource::new()
            .read(path)
            .unwrap_or_else(|_| PartMeta::new(ItemKind::Part));
        let shape = Shape::read_step(path).map_err(|e| module_err(Self::ID, e))?;
        let p = shape.props();
        let g = &mut meta.geometry;
        g.volume_mm3 = Some(p.volume);
        g.area_mm2 = Some(p.area);
        g.center_of_mass = (p.volume > 0.0).then_some(p.center);
        g.bbox = p.bbox.map(|(min, max)| BoundingBox { min, max });
        g.bodies = Some(p.solids);
        g.faces = Some(p.faces);
        meta.source.module = Some(Self::ID.into());
        Ok(meta)
    }
}
