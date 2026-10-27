//! Сцена ↔ glTF 2.0 (`.glb`).
//!
//! Каждое место — узел со своей сеткой. В `extras` узла — роль и объём, в
//! `asset.extras.nonius_scene` — номер формата. Оси не меняются (у glTF, как у
//! SolidWorks, Y вверх), единицы glTF — метры: мм делятся на 1000.

mod read;
mod write;

use serde_json::Value;

use crate::FORMAT_VERSION;

/// Ошибка чтения сцены.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SceneError {
    /// Файл не glTF или испорчен.
    #[error("файл сцены испорчен или это не glTF: {0}")]
    Broken(String),
    /// Сцена записана более новой программой.
    #[error("сцена записана более новой версией Nonius (формат {found}, эта программа понимает до {FORMAT_VERSION}) — обновите программу")]
    TooNew {
        /// Номер формата в файле.
        found: u64,
    },
    /// В файле то, что Nonius не показывает.
    #[error("в сцене есть то, что программа не умеет показать: {0}")]
    Unsupported(String),
}

pub(super) const GLB_MAGIC: u32 = 0x4654_6C67; // "glTF"
pub(super) const CHUNK_JSON: u32 = 0x4E4F_534A; // "JSON"
pub(super) const CHUNK_BIN: u32 = 0x004E_4942; // "BIN\0"
pub(super) const FLOAT: u32 = 5126;
pub(super) const UNSIGNED_INT: u32 = 5125;
pub(super) const ARRAY_BUFFER: u32 = 34962;
pub(super) const ELEMENT_ARRAY_BUFFER: u32 = 34963;

/// мм → м.
pub(super) fn to_gltf(p: [f32; 3]) -> [f32; 3] {
    [p[0] * 1e-3, p[1] * 1e-3, p[2] * 1e-3]
}

/// м → мм.
pub(super) fn from_gltf(p: [f32; 3]) -> [f32; 3] {
    [p[0] * 1e3, p[1] * 1e3, p[2] * 1e3]
}

/// Номер формата сцены из `asset.extras.nonius_scene`; нет — не сцена Nonius.
pub(crate) fn format_version(gltf: &gltf::Gltf) -> Option<u64> {
    extras_json(&gltf.document.as_json().asset.extras)
        .get("nonius_scene")
        .and_then(Value::as_u64)
}

pub(super) fn extras_json(extras: &gltf::json::Extras) -> Value {
    extras
        .as_ref()
        .and_then(|raw| serde_json::from_str(raw.get()).ok())
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_round_trip() {
        let p = [10.0, 20.0, 30.0];
        let q = to_gltf(p);
        assert!((q[1] - 0.02).abs() < 1e-7 && (q[2] - 0.03).abs() < 1e-7);
        let back = from_gltf(q);
        for i in 0..3 {
            assert!((back[i] - p[i]).abs() < 1e-3);
        }
    }
}
