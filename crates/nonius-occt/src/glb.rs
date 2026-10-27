//! Объёмная модель изменений в формате glTF (`.glb`): её показывает окно
//! просмотра, а также открывают «Средство 3D-просмотра» Windows, Blender и
//! любые просмотрщики glTF.
//!
//! Каждый слой — отдельный узел со своим цветом. В `extras` узла лежат
//! роль слоя (`unchanged`, `added`, `removed`), объём и число кусков — по ним
//! окно просмотра строит легенду.
//!
//! Оси: в CAD вверх смотрит Z, в glTF — Y; точки переводятся `(x, y, z) →
//! (x, z, −y)`. Единицы glTF — метры, поэтому миллиметры делятся на 1000.

use serde_json::{json, Value};

use crate::render::Mesh;

/// Слой модели.
#[derive(Debug, Clone, Copy)]
pub struct GlbLayer<'a> {
    /// Название для человека: «Добавлено».
    pub name: &'a str,
    /// Роль для программы: `unchanged`, `added`, `removed`.
    pub role: &'a str,
    /// Сетка, мм.
    pub mesh: &'a Mesh,
    /// Цвет RGB.
    pub color: [u8; 3],
    /// Непрозрачность 0…1; меньше 1 — полупрозрачный «рентген».
    pub opacity: f32,
    /// Объём слоя, мм³.
    pub volume_mm3: f64,
    /// Число отдельных кусков.
    pub pieces: u32,
}

const GLB_MAGIC: u32 = 0x4654_6C67; // "glTF"
const CHUNK_JSON: u32 = 0x4E4F_534A; // "JSON"
const CHUNK_BIN: u32 = 0x004E_4942; // "BIN\0"
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;

/// CAD (мм, Z вверх) → glTF (м, Y вверх).
fn to_gltf(p: [f32; 3]) -> [f32; 3] {
    [p[0] * 1e-3, p[2] * 1e-3, -p[1] * 1e-3]
}

fn srgb_to_linear(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Собрать `.glb` из слоёв. Пустые слои пропускаются; без слоёв получается
/// пустая сцена.
pub fn to_glb(layers: &[GlbLayer<'_>]) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut materials = Vec::new();
    let mut nodes = Vec::new();

    for layer in layers.iter().filter(|l| !l.mesh.is_empty()) {
        let mesh = layer.mesh;
        let count = mesh.positions.len() as u32;

        // Вершины.
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

        // Треугольники; битые индексы отбрасываются.
        let idx_offset = bin.len();
        let mut idx_count = 0u32;
        for t in mesh
            .triangles
            .iter()
            .filter(|t| t.iter().all(|&i| i < count))
        {
            for &i in t {
                bin.extend_from_slice(&i.to_le_bytes());
            }
            idx_count += 3;
        }
        let idx_len = bin.len() - idx_offset;
        if idx_count == 0 {
            bin.truncate(pos_offset);
            continue;
        }

        let pos_view = views.len();
        views.push(json!({
            "buffer": 0, "byteOffset": pos_offset, "byteLength": pos_len,
            "target": ARRAY_BUFFER,
        }));
        views.push(json!({
            "buffer": 0, "byteOffset": idx_offset, "byteLength": idx_len,
            "target": ELEMENT_ARRAY_BUFFER,
        }));
        let pos_acc = accessors.len();
        accessors.push(json!({
            "bufferView": pos_view, "componentType": FLOAT, "count": count,
            "type": "VEC3", "min": lo, "max": hi,
        }));
        accessors.push(json!({
            "bufferView": pos_view + 1, "componentType": UNSIGNED_INT,
            "count": idx_count, "type": "SCALAR",
        }));

        let [r, g, b] = layer.color;
        let opacity = layer.opacity.clamp(0.0, 1.0);
        let mut material = json!({
            "name": layer.role,
            "doubleSided": true,
            "pbrMetallicRoughness": {
                "baseColorFactor": [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), opacity],
                "metallicFactor": 0.0,
                "roughnessFactor": 0.7,
            },
        });
        if opacity < 1.0 {
            material["alphaMode"] = json!("BLEND");
        }
        materials.push(material);

        meshes.push(json!({
            "name": layer.name,
            "primitives": [{
                "attributes": { "POSITION": pos_acc },
                "indices": pos_acc + 1,
                "material": materials.len() - 1,
            }],
        }));
        nodes.push(json!({
            "name": layer.name,
            "mesh": meshes.len() - 1,
            "extras": {
                "role": layer.role,
                "volume_mm3": layer.volume_mm3,
                "pieces": layer.pieces,
            },
        }));
    }

    let mut doc = json!({
        "asset": { "version": "2.0", "generator": "Nonius" },
        "scene": 0,
        "scenes": [{ "nodes": (0..nodes.len()).collect::<Vec<_>>() }],
    });
    if !nodes.is_empty() {
        doc["nodes"] = Value::Array(nodes);
        doc["meshes"] = Value::Array(meshes);
        doc["materials"] = Value::Array(materials);
        doc["accessors"] = Value::Array(accessors);
        doc["bufferViews"] = Value::Array(views);
        doc["buffers"] = json!([{ "byteLength": bin.len() }]);
    }

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

/// Разобрать `.glb` обратно: JSON сцены и двоичная часть. Нужен для проверок.
pub fn parse_glb(data: &[u8]) -> Option<(Value, Vec<u8>)> {
    let word = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
    };
    if word(0)? != GLB_MAGIC || word(4)? != 2 || word(8)? as usize != data.len() {
        return None;
    }
    let json_len = word(12)? as usize;
    if word(16)? != CHUNK_JSON {
        return None;
    }
    let doc = serde_json::from_slice(data.get(20..20 + json_len)?).ok()?;
    let rest = 20 + json_len;
    let bin = if rest < data.len() {
        let bin_len = word(rest)? as usize;
        if word(rest + 4)? != CHUNK_BIN {
            return None;
        }
        data.get(rest + 8..rest + 8 + bin_len)?.to_vec()
    } else {
        Vec::new()
    };
    Some((doc, bin))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> Mesh {
        Mesh {
            positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 20.0, 30.0]],
            triangles: vec![[0, 1, 2]],
        }
    }

    fn layer<'a>(mesh: &'a Mesh, role: &'a str, opacity: f32) -> GlbLayer<'a> {
        GlbLayer {
            name: "Слой",
            role,
            mesh,
            color: [34, 170, 76],
            opacity,
            volume_mm3: 123.5,
            pieces: 2,
        }
    }

    #[test]
    fn round_trip() {
        let mesh = triangle();
        let glb = to_glb(&[layer(&mesh, "unchanged", 0.35), layer(&mesh, "added", 1.0)]);
        assert_eq!(glb.len() % 4, 0);
        let (doc, bin) = parse_glb(&glb).expect("корректный GLB");
        assert_eq!(doc["asset"]["version"], "2.0");
        assert_eq!(doc["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(doc["nodes"][1]["extras"]["role"], "added");
        assert_eq!(doc["nodes"][1]["extras"]["volume_mm3"], 123.5);
        assert_eq!(doc["nodes"][1]["extras"]["pieces"], 2);
        assert_eq!(doc["materials"][0]["alphaMode"], "BLEND");
        assert!(doc["materials"][1].get("alphaMode").is_none());
        // 2 слоя × (3 вершины × 12 байт + 3 индекса × 4 байта)
        assert_eq!(bin.len(), 2 * (36 + 12));
        assert_eq!(doc["buffers"][0]["byteLength"], 96);
    }

    #[test]
    fn axes_and_units() {
        let mesh = triangle();
        let (doc, bin) = parse_glb(&to_glb(&[layer(&mesh, "added", 1.0)])).unwrap();
        let acc = &doc["accessors"][0];
        assert_eq!(acc["count"], 3);
        // CAD (0, 20, 30) мм → glTF (0, 0.03, −0.02) м.
        let f = |i: usize| f32::from_le_bytes(bin[i * 4..i * 4 + 4].try_into().unwrap());
        let third = [f(6), f(7), f(8)];
        assert!((third[0] - 0.0).abs() < 1e-7);
        assert!((third[1] - 0.03).abs() < 1e-7);
        assert!((third[2] + 0.02).abs() < 1e-7);
        assert!((acc["max"][1].as_f64().unwrap() - 0.03).abs() < 1e-6);
        assert!((acc["min"][2].as_f64().unwrap() + 0.02).abs() < 1e-6);
    }

    #[test]
    fn empty_and_broken_layers_are_skipped() {
        let empty = Mesh::default();
        let broken = Mesh {
            positions: vec![[0.0; 3]],
            triangles: vec![[0, 5, 9]],
        };
        let glb = to_glb(&[layer(&empty, "added", 1.0), layer(&broken, "removed", 1.0)]);
        let (doc, bin) = parse_glb(&glb).unwrap();
        assert!(doc.get("nodes").is_none());
        assert_eq!(doc["scenes"][0]["nodes"].as_array().unwrap().len(), 0);
        assert!(bin.is_empty());
    }

    #[test]
    fn garbage_is_not_glb() {
        assert!(parse_glb(b"not a glb file").is_none());
        assert!(parse_glb(&[]).is_none());
    }
}
