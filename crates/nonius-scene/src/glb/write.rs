//! Сцена → `.glb`.

use serde_json::{json, Value};

use super::{
    to_gltf, ARRAY_BUFFER, CHUNK_BIN, CHUNK_JSON, ELEMENT_ARRAY_BUFFER, FLOAT, GLB_MAGIC,
    UNSIGNED_INT,
};
use crate::{Role, Scene, FORMAT_VERSION};

/// Непрозрачность новой версии, когда в сцене есть изменения: чтобы изменения
/// внутри детали были видны в сторонних просмотрщиках.
const XRAY_OPACITY: f64 = 0.35;

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
