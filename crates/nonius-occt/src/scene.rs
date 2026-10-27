//! Объёмная модель изменений для окна просмотра: `changes.glb`.

use std::path::{Path, PathBuf};

use nonius_core::compare::{ChangeReport, CompareContext};
use nonius_core::extension::{ExtensionPoint, Module, ModuleManifest, Renderer, Snapshot};
use nonius_core::{Error, Result};

use crate::glb::{to_glb, GlbLayer};
use crate::modules::{deflection, manifest, model_pair, module_err, same_bytes, ANGULAR};
use crate::render::{colors, Mesh};
use crate::shape::Shape;
use crate::{GeometryDiff, OcctError, SCENE_RENDERER};

/// Имя модели изменений, которую пишет [`SceneRenderer`].
pub const CHANGES_GLB: &str = "changes.glb";

/// Непрозрачность серого «рентгена», когда есть изменения.
const XRAY_OPACITY: f32 = 0.35;

impl GeometryDiff {
    /// Объёмная модель: серое — новая версия целиком, зелёное — добавлено,
    /// красное — убрано. Без изменений серое непрозрачно.
    pub fn to_glb(&self) -> Vec<u8> {
        let after = self.after.props();
        let defl = deflection(after.bbox.or(self.before.props().bbox));
        let empty = Mesh::default();
        let common = self.after.mesh(defl, ANGULAR);
        let added = if self.has_added() {
            self.added.mesh(defl, ANGULAR)
        } else {
            empty.clone()
        };
        let removed = if self.has_removed() {
            self.removed.mesh(defl, ANGULAR)
        } else {
            empty
        };
        let opacity = if self.changed() { XRAY_OPACITY } else { 1.0 };
        to_glb(&[
            GlbLayer {
                name: "Без изменений",
                role: "unchanged",
                mesh: &common,
                color: colors::UNCHANGED,
                opacity,
                volume_mm3: after.volume,
                pieces: after.solids,
            },
            GlbLayer {
                name: "Добавлено",
                role: "added",
                mesh: &added,
                color: colors::ADDED,
                opacity: 1.0,
                volume_mm3: self.added_volume,
                pieces: self.added_pieces,
            },
            GlbLayer {
                name: "Убрано",
                role: "removed",
                mesh: &removed,
                color: colors::REMOVED,
                opacity: 1.0,
                volume_mm3: self.removed_volume,
                pieces: self.removed_pieces,
            },
        ])
    }
}

/// Объёмная модель одной детали без сравнения.
pub fn shape_glb(shape: &Shape) -> Vec<u8> {
    let p = shape.props();
    let mesh = shape.mesh(deflection(p.bbox), ANGULAR);
    to_glb(&[GlbLayer {
        name: "Деталь",
        role: "unchanged",
        mesh: &mesh,
        color: colors::UNCHANGED,
        opacity: 1.0,
        volume_mm3: p.volume,
        pieces: p.solids,
    }])
}

fn scene_of(before: &Path, after: &Path) -> std::result::Result<Vec<u8>, OcctError> {
    // Одинаковые файлы: булевы операции не нужны, рисуем деталь как есть.
    if same_bytes(before, after) {
        return Ok(shape_glb(&Shape::read_step(after)?));
    }
    let tolerance = CompareContext::default().tolerance_mm;
    Ok(GeometryDiff::of_files(before, after, tolerance)?.to_glb())
}

/// Модель изменений для окна просмотра: `changes.glb` (glTF) с цветными слоями.
pub struct SceneRenderer {
    manifest: ModuleManifest,
}

impl SceneRenderer {
    /// Идентификатор модуля.
    pub const ID: &'static str = SCENE_RENDERER;

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: manifest(
                Self::ID,
                "Объёмная модель изменений (glTF)",
                ExtensionPoint::Renderer,
            ),
        }
    }
}

impl Default for SceneRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for SceneRenderer {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl Renderer for SceneRenderer {
    fn render(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        _report: &ChangeReport,
        out_dir: &Path,
    ) -> Result<Vec<PathBuf>> {
        let Some((a, b)) = model_pair(before, after) else {
            return Ok(Vec::new());
        };
        let glb = scene_of(&a, &b).map_err(|e| module_err(Self::ID, e))?;
        std::fs::create_dir_all(out_dir).map_err(|e| Error::Io {
            path: out_dir.to_path_buf(),
            source: e,
        })?;
        let path = out_dir.join(CHANGES_GLB);
        std::fs::write(&path, glb).map_err(|e| Error::Io {
            path: path.clone(),
            source: e,
        })?;
        Ok(vec![path])
    }
}
