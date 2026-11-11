//! Сцена изменений для окна: `changes.glb` (формат `nonius-scene`).

use std::path::{Path, PathBuf};

use nonius_core::compare::{ChangeReport, CompareContext};
use nonius_core::extension::{ExtensionPoint, Module, ModuleManifest, Renderer, Snapshot};
use nonius_core::{Error, Result};
use nonius_scene::{Mesh, Region, Role, Scene};

use super::{
    deflection, manifest, model_pair, module_err, same_bytes, shared_diff, GeometryDiff, Piece,
    ANGULAR,
};
use crate::render;
use crate::shape::Shape;
use crate::{OcctError, CHANGES_GLB, SCENE_RENDERER};

fn to_scene_mesh(m: render::Mesh) -> Mesh {
    Mesh {
        positions: m.positions,
        triangles: m.triangles,
    }
}

/// Места одной роли: каждый кусок — отдельное место.
fn places(pieces: &[Piece], role: Role, defl: f64, out: &mut Vec<Region>) {
    for piece in pieces {
        let mesh = to_scene_mesh(piece.shape.mesh(defl, ANGULAR));
        if !mesh.is_empty() {
            out.push(Region {
                role,
                volume_mm3: piece.volume,
                mesh,
            });
        }
    }
}

impl GeometryDiff {
    /// Сцена изменений: новая и старая версии целиком, зелёное и красное —
    /// каждое добавленное и убранное тело отдельным местом, крупные первыми.
    pub fn scene(&self) -> Scene {
        // Объёмы уже посчитаны при сравнении, габариты нужны только для
        // допуска сетки: полный паспорт тела здесь — лишние секунды.
        let defl = deflection(self.after.rough_bbox().or_else(|| self.before.rough_bbox()));
        let mut regions = vec![Region {
            role: Role::After,
            volume_mm3: self.after.volume().max(0.0),
            mesh: to_scene_mesh(self.after.mesh(defl, ANGULAR)),
        }];
        if self.changed() {
            regions.push(Region {
                role: Role::Before,
                volume_mm3: self.before.volume().max(0.0),
                mesh: to_scene_mesh(self.before.mesh(defl, ANGULAR)),
            });
        }
        let mut changes = Vec::new();
        // Шум булевых операций сюда не попадает: он отсеян при сравнении.
        places(&self.added, Role::Added, defl, &mut changes);
        places(&self.removed, Role::Removed, defl, &mut changes);
        changes.sort_by(|a, b| b.volume_mm3.total_cmp(&a.volume_mm3));
        regions.extend(changes);
        Scene { regions }
    }
}

/// Сцена одной детали без сравнения.
pub fn shape_scene(shape: &Shape) -> Scene {
    Scene {
        regions: vec![Region {
            role: Role::After,
            volume_mm3: shape.volume().max(0.0),
            mesh: to_scene_mesh(shape.mesh(deflection(shape.rough_bbox()), ANGULAR)),
        }],
    }
}

fn scene_of(
    before: &Path,
    after: &Path,
    tolerance_mm: f64,
) -> std::result::Result<Scene, OcctError> {
    // Одинаковые файлы: булевы операции не нужны, показываем деталь как есть.
    if same_bytes(before, after) {
        return Ok(shape_scene(&Shape::read_step(after)?));
    }
    Ok(shared_diff(before, after, tolerance_mm)?.scene())
}

/// Сцена изменений для окна: `changes.glb` с местами изменений.
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
                "Сцена изменений для окна",
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
        report: &ChangeReport,
        out_dir: &Path,
    ) -> Result<Vec<PathBuf>> {
        self.render_with(before, after, report, &CompareContext::default(), out_dir)
    }

    /// С допуском проекта: та же разница, что в отчёте сравнения, — и она
    /// не считается второй раз.
    fn render_with(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        _report: &ChangeReport,
        ctx: &CompareContext,
        out_dir: &Path,
    ) -> Result<Vec<PathBuf>> {
        let Some((a, b)) = model_pair(before, after) else {
            return Ok(Vec::new());
        };
        let scene = scene_of(&a, &b, ctx.tolerance_mm).map_err(|e| module_err(Self::ID, e))?;
        std::fs::create_dir_all(out_dir).map_err(|e| Error::Io {
            path: out_dir.to_path_buf(),
            source: e,
        })?;
        let path = out_dir.join(CHANGES_GLB);
        std::fs::write(&path, scene.to_glb()).map_err(|e| Error::Io {
            path: path.clone(),
            source: e,
        })?;
        Ok(vec![path])
    }
}
