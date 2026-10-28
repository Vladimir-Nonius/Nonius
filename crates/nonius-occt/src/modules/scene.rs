//! Сцена изменений для окна: `changes.glb` (формат `nonius-scene`).

use std::path::{Path, PathBuf};

use nonius_core::compare::{ChangeReport, CompareContext};
use nonius_core::extension::{ExtensionPoint, Module, ModuleManifest, Renderer, Snapshot};
use nonius_core::{Error, Result};
use nonius_scene::{Mesh, Region, Role, Scene};

use super::{deflection, manifest, model_pair, module_err, same_bytes, GeometryDiff, ANGULAR};
use crate::render;
use crate::shape::Shape;
use crate::{OcctError, CHANGES_GLB, SCENE_RENDERER};

fn to_scene_mesh(m: render::Mesh) -> Mesh {
    Mesh {
        positions: m.positions,
        triangles: m.triangles,
    }
}

/// Места одной роли: каждое твёрдое тело — отдельное место.
fn places(shape: &Shape, role: Role, defl: f64, out: &mut Vec<Region>) {
    for solid in shape.solids() {
        let volume = solid.props().volume;
        let mesh = to_scene_mesh(solid.mesh(defl, ANGULAR));
        if !mesh.is_empty() {
            out.push(Region {
                role,
                volume_mm3: volume.max(0.0),
                mesh,
            });
        }
    }
}

impl GeometryDiff {
    /// Сцена изменений: новая и старая версии целиком, зелёное и красное —
    /// каждое добавленное и убранное тело отдельным местом, крупные первыми.
    pub fn scene(&self) -> Scene {
        let after = self.after.props();
        let before = self.before.props();
        let defl = deflection(after.bbox.or(before.bbox));
        let mut regions = vec![Region {
            role: Role::After,
            volume_mm3: after.volume.max(0.0),
            mesh: to_scene_mesh(self.after.mesh(defl, ANGULAR)),
        }];
        if self.changed() {
            regions.push(Region {
                role: Role::Before,
                volume_mm3: before.volume.max(0.0),
                mesh: to_scene_mesh(self.before.mesh(defl, ANGULAR)),
            });
        }
        let mut changes = Vec::new();
        if self.has_added() {
            places(&self.added, Role::Added, defl, &mut changes);
        }
        if self.has_removed() {
            places(&self.removed, Role::Removed, defl, &mut changes);
        }
        // Шум булевых операций (крошки меньше порога) местом не считается.
        changes.retain(|r| r.volume_mm3 > self.threshold);
        changes.sort_by(|a, b| b.volume_mm3.total_cmp(&a.volume_mm3));
        regions.extend(changes);
        Scene { regions }
    }
}

/// Сцена одной детали без сравнения.
pub fn shape_scene(shape: &Shape) -> Scene {
    let p = shape.props();
    Scene {
        regions: vec![Region {
            role: Role::After,
            volume_mm3: p.volume.max(0.0),
            mesh: to_scene_mesh(shape.mesh(deflection(p.bbox), ANGULAR)),
        }],
    }
}

fn scene_of(before: &Path, after: &Path) -> std::result::Result<Scene, OcctError> {
    // Одинаковые файлы: булевы операции не нужны, показываем деталь как есть.
    if same_bytes(before, after) {
        return Ok(shape_scene(&Shape::read_step(after)?));
    }
    let tolerance = CompareContext::default().tolerance_mm;
    Ok(GeometryDiff::of_files(before, after, tolerance)?.scene())
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
        _report: &ChangeReport,
        out_dir: &Path,
    ) -> Result<Vec<PathBuf>> {
        let Some((a, b)) = model_pair(before, after) else {
            return Ok(Vec::new());
        };
        let scene = scene_of(&a, &b).map_err(|e| module_err(Self::ID, e))?;
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
