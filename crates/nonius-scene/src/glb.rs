//! Сцена ↔ glTF 2.0 (`.glb`).
//!
//! Каждое место — узел со своей сеткой. В `extras` узла — роль и объём, в
//! `asset.extras.nonius_scene` — номер формата. Оси не меняются (у glTF, как у
//! SolidWorks, Y вверх), единицы glTF — метры: мм делятся на 1000.

use serde_json::{json, Value};

use crate::{Mesh, Region, Role, Scene, FORMAT_VERSION};

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

const GLB_MAGIC: u32 = 0x4654_6C67; // "glTF"
const CHUNK_JSON: u32 = 0x4E4F_534A; // "JSON"
const CHUNK_BIN: u32 = 0x004E_4942; // "BIN\0"
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;

/// Непрозрачность новой версии, когда в сцене есть изменения: чтобы изменения
/// внутри детали были видны в сторонних просмотрщиках.
const XRAY_OPACITY: f64 = 0.35;

/// мм → м.
fn to_gltf(p: [f32; 3]) -> [f32; 3] {
    [p[0] * 1e-3, p[1] * 1e-3, p[2] * 1e-3]
}

/// м → мм.
fn from_gltf(p: [f32; 3]) -> [f32; 3] {
    [p[0] * 1e3, p[1] * 1e3, p[2] * 1e3]
}

fn srgb_to_linear(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::After => "Стало",
        Role::Before => "Было",
        Role::Added => "Добавлено",
        Role::Removed => "Убрано",
        Role::Moved => "Сдвинуто",
    }
}

impl Scene {
    /// Записать сцену в `.glb`. Пустые места пропускаются.
    pub fn to_glb(&self) -> Vec<u8> {
        let changed = self.has_changes();
        let mut bin: Vec<u8> = Vec::new();
        let mut views = Vec::new();
        let mut accessors = Vec::new();
        let mut meshes = Vec::new();
        let mut materials: Vec<Value> = Vec::new();
        let mut material_of: Vec<(Role, usize)> = Vec::new();
        let mut nodes = Vec::new();
        let mut places = [0u32; 5];

        for region in self.regions.iter().filter(|r| !r.mesh.is_empty()) {
            let mesh = &region.mesh;
            let count = mesh.positions.len() as u32;
            let triangles: Vec<&[u32; 3]> = mesh
                .triangles
                .iter()
                .filter(|t| t.iter().all(|&i| i < count))
                .collect();
            if triangles.is_empty() {
                continue;
            }

            let mut lo = [f32::INFINITY; 3];
            let mut hi = [f32::NEG_INFINITY; 3];
            let pos_offset = bin.len();
            for &p in &mesh.positions {
                let q = to_gltf(p);
                for i in 0..3 {
                    lo[i] = lo[i].min(q[i]);
                    hi[i] = hi[i].max(q[i]);
                    bin.extend_from_slice(&q[i].to_le_bytes());
                }
            }
            let pos_len = bin.len() - pos_offset;
            let idx_offset = bin.len();
            for t in &triangles {
                for &i in t.iter() {
                    bin.extend_from_slice(&i.to_le_bytes());
                }
            }
            let idx_len = bin.len() - idx_offset;

            let view = views.len();
            views.push(json!({
                "buffer": 0, "byteOffset": pos_offset, "byteLength": pos_len,
                "target": ARRAY_BUFFER,
            }));
            views.push(json!({
                "buffer": 0, "byteOffset": idx_offset, "byteLength": idx_len,
                "target": ELEMENT_ARRAY_BUFFER,
            }));
            let acc = accessors.len();
            accessors.push(json!({
                "bufferView": view, "componentType": FLOAT, "count": count,
                "type": "VEC3", "min": lo, "max": hi,
            }));
            accessors.push(json!({
                "bufferView": view + 1, "componentType": UNSIGNED_INT,
                "count": triangles.len() * 3, "type": "SCALAR",
            }));

            let material = match material_of.iter().find(|(r, _)| *r == region.role) {
                Some(&(_, m)) => m,
                None => {
                    let [r, g, b] = region.role.color();
                    let opacity = if region.role == Role::After && changed {
                        XRAY_OPACITY
                    } else {
                        1.0
                    };
                    let mut m = json!({
                        "name": region.role.as_str(),
                        "doubleSided": true,
                        "pbrMetallicRoughness": {
                            "baseColorFactor": [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), opacity],
                            "metallicFactor": 0.0,
                            "roughnessFactor": 0.7,
                        },
                    });
                    if opacity < 1.0 {
                        m["alphaMode"] = json!("BLEND");
                    }
                    materials.push(m);
                    material_of.push((region.role, materials.len() - 1));
                    materials.len() - 1
                }
            };

            let place = &mut places[region.role as usize];
            *place += 1;
            let name = format!("{} {}", role_name(region.role), place);
            meshes.push(json!({
                "name": name,
                "primitives": [{
                    "attributes": { "POSITION": acc },
                    "indices": acc + 1,
                    "material": material,
                }],
            }));
            nodes.push(json!({
                "name": name,
                "mesh": meshes.len() - 1,
                "extras": { "role": region.role.as_str(), "volume_mm3": region.volume_mm3 },
            }));
        }

        // Старая версия целиком нужна окну для режима «было», а сторонним
        // просмотрщикам мешала бы. Поэтому она во второй сцене glTF: они
        // показывают только сцену по умолчанию, окно Nonius читает обе.
        let is_before = |n: &Value| n["extras"]["role"] == Role::Before.as_str();
        let main: Vec<usize> = (0..nodes.len())
            .filter(|&i| !is_before(&nodes[i]))
            .collect();
        let before: Vec<usize> = (0..nodes.len()).filter(|&i| is_before(&nodes[i])).collect();
        let mut scenes = vec![json!({ "name": "Сравнение", "nodes": main })];
        if !before.is_empty() {
            scenes.push(json!({ "name": "Было", "nodes": before }));
        }
        let mut doc = json!({
            "asset": {
                "version": "2.0",
                "generator": "Nonius",
                "extras": { "nonius_scene": FORMAT_VERSION },
            },
            "scene": 0,
            "scenes": scenes,
        });
        if !nodes.is_empty() {
            doc["nodes"] = Value::Array(nodes);
            doc["meshes"] = Value::Array(meshes);
            doc["materials"] = Value::Array(materials);
            doc["accessors"] = Value::Array(accessors);
            doc["bufferViews"] = Value::Array(views);
            doc["buffers"] = json!([{ "byteLength": bin.len() }]);
        }
        glb_container(doc, bin)
    }

    /// Прочитать сцену из `.glb`. Понимает и glTF сторонних программ: у них
    /// всё считается деталью целиком (`after`).
    pub fn from_glb(data: &[u8]) -> Result<Scene, SceneError> {
        let gltf = gltf::Gltf::from_slice(data).map_err(|e| SceneError::Broken(e.to_string()))?;
        if let Some(found) = format_version(&gltf) {
            if found > FORMAT_VERSION as u64 {
                return Err(SceneError::TooNew { found });
            }
        }
        let blob = gltf.blob.as_deref();
        let buffer = |b: gltf::Buffer<'_>| -> Option<&[u8]> {
            match b.source() {
                gltf::buffer::Source::Bin => blob,
                gltf::buffer::Source::Uri(_) => None,
            }
        };

        let mut scene = Scene::default();
        // Все сцены glTF по порядку, каждый узел один раз.
        let mut seen = vec![false; gltf.nodes().count()];
        let mut stack: Vec<(gltf::Node<'_>, [[f32; 4]; 4])> = gltf
            .scenes()
            .flat_map(|s| s.nodes().collect::<Vec<_>>())
            .map(|n| (n, IDENTITY))
            .collect();
        stack.reverse();
        while let Some((node, parent)) = stack.pop() {
            if std::mem::replace(&mut seen[node.index()], true) {
                continue;
            }
            let world = mul(parent, node.transform().matrix());
            if let Some(mesh) = node.mesh() {
                let extras = extras_json(node.extras());
                let role = extras
                    .get("role")
                    .and_then(Value::as_str)
                    .and_then(Role::parse)
                    .unwrap_or(Role::After);
                let volume_mm3 = extras
                    .get("volume_mm3")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let mut out = Mesh::default();
                for prim in mesh.primitives() {
                    if prim.mode() != gltf::mesh::Mode::Triangles {
                        return Err(SceneError::Unsupported("сетка не из треугольников".into()));
                    }
                    let reader = prim.reader(buffer);
                    let Some(positions) = reader.read_positions() else {
                        continue;
                    };
                    let base = out.positions.len() as u32;
                    out.positions
                        .extend(positions.map(|p| from_gltf(apply(&world, p))));
                    let count = out.positions.len() as u32 - base;
                    let indices: Vec<u32> = match reader.read_indices() {
                        Some(ix) => ix.into_u32().collect(),
                        None => (0..count).collect(),
                    };
                    for t in indices.chunks_exact(3) {
                        if t.iter().all(|&i| i < count) {
                            out.triangles.push([base + t[0], base + t[1], base + t[2]]);
                        }
                    }
                }
                if !out.is_empty() {
                    scene.regions.push(Region {
                        role,
                        volume_mm3,
                        mesh: out,
                    });
                }
            }
            let children: Vec<_> = node.children().map(|c| (c, world)).collect();
            stack.extend(children.into_iter().rev());
        }
        Ok(scene)
    }
}

/// Номер формата сцены из `asset.extras.nonius_scene`; нет — не сцена Nonius.
pub(crate) fn format_version(gltf: &gltf::Gltf) -> Option<u64> {
    extras_json(&gltf.document.as_json().asset.extras)
        .get("nonius_scene")
        .and_then(Value::as_u64)
}

fn extras_json(extras: &gltf::json::Extras) -> Value {
    extras
        .as_ref()
        .and_then(|raw| serde_json::from_str(raw.get()).ok())
        .unwrap_or(Value::Null)
}

const IDENTITY: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Произведение матриц glTF (по столбцам): `a · b`.
fn mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut m = [[0.0; 4]; 4];
    for (c, col) in m.iter_mut().enumerate() {
        for (r, v) in col.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    m
}

fn apply(m: &[[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (r, v) in out.iter_mut().enumerate() {
        *v = m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r];
    }
    out
}

/// Упаковать JSON и двоичную часть в контейнер `.glb`.
fn glb_container(doc: Value, mut bin: Vec<u8>) -> Vec<u8> {
    let mut json_chunk = serde_json::to_vec(&doc).expect("JSON сцены всегда сериализуется");
    while json_chunk.len() % 4 != 0 {
        json_chunk.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let has_bin = !bin.is_empty();
    let total = 12 + 8 + json_chunk.len() + if has_bin { 8 + bin.len() } else { 0 };
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&GLB_MAGIC.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
    out.extend_from_slice(&json_chunk);
    if has_bin {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
        out.extend_from_slice(&bin);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrices() {
        let shift = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [1.0, 2.0, 3.0, 1.0],
        ];
        assert_eq!(mul(IDENTITY, shift), shift);
        assert_eq!(apply(&mul(shift, shift), [0.0; 3]), [2.0, 4.0, 6.0]);
    }

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
