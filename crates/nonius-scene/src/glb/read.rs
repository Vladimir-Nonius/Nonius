//! `.glb` → сцена.

use serde_json::Value;

use super::{extras_json, format_version, from_gltf, SceneError};
use crate::{Mesh, Region, Role, Scene, FORMAT_VERSION};

impl Scene {
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
}
