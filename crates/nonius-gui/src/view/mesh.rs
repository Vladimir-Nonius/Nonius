//! Подготовка сетки к отрисовке: нормали для света и рёбра детали.
//!
//! OpenCascade режет каждую грань отдельно, поэтому вершины на границах граней
//! не общие: сглаживание нормалей по общим вершинам не размывает углы детали.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use nonius_scene::{Bounds, Mesh, Region, Role, Scene};

/// Вершина для видеокарты.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    /// Положение, мм.
    pub pos: [f32; 3],
    /// Нормаль для света.
    pub normal: [f32; 3],
}

/// Угол между гранями, начиная с которого рисуется ребро.
const EDGE_ANGLE_DEG: f32 = 25.0;

/// Кусок сцены, готовый к отрисовке.
#[derive(Debug, Clone)]
pub struct Part {
    /// Что это: деталь целиком или место изменения.
    pub role: Role,
    /// Номер места среди изменений той же роли, с 1; у детали целиком — 0.
    pub number: usize,
    /// Объём, мм³.
    pub volume_mm3: f64,
    /// Вершины.
    pub vertices: Vec<Vertex>,
    /// Треугольники, по три номера вершин.
    pub indices: Vec<u32>,
    /// Рёбра: пары точек.
    pub edges: Vec<[f32; 3]>,
    /// Габариты.
    pub bounds: Bounds,
}

impl Part {
    /// Подготовить место `region`.
    pub fn new(region: &Region, number: usize) -> Option<Self> {
        let bounds = region.mesh.bounds()?;
        Some(Self {
            role: region.role,
            number,
            volume_mm3: region.volume_mm3,
            vertices: vertices(&region.mesh),
            indices: region.mesh.triangles.iter().flatten().copied().collect(),
            edges: edges(&region.mesh, bounds.diagonal()),
            bounds,
        })
    }

    /// Треугольники в виде точек.
    pub fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        self.indices
            .chunks_exact(3)
            .map(|t| [t[0], t[1], t[2]].map(|i| Vec3::from(self.vertices[i as usize].pos)))
    }
}

/// Подготовить всю сцену. Места изменений нумеруются по ролям в порядке сцены.
pub fn prepare(scene: &Scene) -> Vec<Part> {
    let mut counts: HashMap<Role, usize> = HashMap::new();
    scene
        .regions
        .iter()
        .filter(|r| !r.mesh.is_empty())
        .filter_map(|r| {
            let number = if r.role.is_change() {
                let n = counts.entry(r.role).or_default();
                *n += 1;
                *n
            } else {
                0
            };
            Part::new(r, number)
        })
        .collect()
}

fn face_normal(m: &Mesh, t: &[u32; 3]) -> Vec3 {
    let [a, b, c] = t.map(|i| Vec3::from(m.positions[i as usize]));
    (b - a).cross(c - a)
}

/// Нормали вершин: сумма нормалей соседних треугольников с весом по площади.
fn vertices(m: &Mesh) -> Vec<Vertex> {
    let mut normals = vec![Vec3::ZERO; m.positions.len()];
    for t in &m.triangles {
        let n = face_normal(m, t);
        for &i in t {
            normals[i as usize] += n;
        }
    }
    m.positions
        .iter()
        .zip(normals)
        .map(|(&pos, n)| Vertex {
            pos,
            normal: n.try_normalize().unwrap_or(Vec3::Z).into(),
        })
        .collect()
}

/// Рёбра детали: где грани сходятся под углом и где у поверхности край.
/// Точки сшиваются по положению, потому что грани не делят вершины.
fn edges(m: &Mesh, size: f32) -> Vec<[f32; 3]> {
    let step = (size * 1e-5).max(1e-6);
    let mut weld: HashMap<[i64; 3], u32> = HashMap::new();
    let ids: Vec<u32> = m
        .positions
        .iter()
        .map(|p| {
            let key = p.map(|c| (c / step).round() as i64);
            let next = weld.len() as u32;
            *weld.entry(key).or_insert(next)
        })
        .collect();

    // Ребро (по сшитым точкам) → нормали треугольников и одна пара исходных точек.
    let mut sides: HashMap<(u32, u32), (Vec<Vec3>, [u32; 2])> = HashMap::new();
    for t in &m.triangles {
        let n = face_normal(m, t).normalize_or_zero();
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let (wa, wb) = (ids[a as usize], ids[b as usize]);
            if wa == wb {
                continue;
            }
            let key = (wa.min(wb), wa.max(wb));
            sides
                .entry(key)
                .or_insert_with(|| (Vec::new(), [a, b]))
                .0
                .push(n);
        }
    }

    let cos_limit = EDGE_ANGLE_DEG.to_radians().cos();
    let mut out = Vec::new();
    let mut keys: Vec<_> = sides.keys().copied().collect();
    keys.sort_unstable();
    for key in keys {
        let (normals, [a, b]) = &sides[&key];
        let sharp = match normals.as_slice() {
            [x, y] => x.dot(*y) < cos_limit,
            _ => true,
        };
        if sharp {
            out.push(m.positions[*a as usize]);
            out.push(m.positions[*b as usize]);
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Куб 10 мм, как его режет OpenCascade: у каждой грани свои 4 вершины.
    pub(crate) fn cube_mesh(at: Vec3, size: f32) -> Mesh {
        let mut m = Mesh::default();
        let corner =
            |x: u8, y: u8, z: u8| (at + Vec3::new(x as f32, y as f32, z as f32) * size).to_array();
        let faces: [[(u8, u8, u8); 4]; 6] = [
            [(0, 0, 0), (0, 1, 0), (1, 1, 0), (1, 0, 0)],
            [(0, 0, 1), (1, 0, 1), (1, 1, 1), (0, 1, 1)],
            [(0, 0, 0), (1, 0, 0), (1, 0, 1), (0, 0, 1)],
            [(0, 1, 0), (0, 1, 1), (1, 1, 1), (1, 1, 0)],
            [(0, 0, 0), (0, 0, 1), (0, 1, 1), (0, 1, 0)],
            [(1, 0, 0), (1, 1, 0), (1, 1, 1), (1, 0, 1)],
        ];
        for f in faces {
            let base = m.positions.len() as u32;
            m.positions.extend(f.map(|(x, y, z)| corner(x, y, z)));
            m.triangles.push([base, base + 1, base + 2]);
            m.triangles.push([base, base + 2, base + 3]);
        }
        m
    }

    fn region(role: Role, at: Vec3) -> Region {
        Region {
            role,
            volume_mm3: 1000.0,
            mesh: cube_mesh(at, 10.0),
        }
    }

    #[test]
    fn cube_has_twelve_edges_and_flat_normals() {
        let p = Part::new(&region(Role::After, Vec3::ZERO), 0).unwrap();
        assert_eq!(p.edges.len(), 12 * 2, "диагонали граней — не рёбра");
        assert_eq!(p.indices.len(), 36);
        for v in &p.vertices {
            let n = Vec3::from(v.normal);
            let axis = n.abs().max_element();
            assert!((axis - 1.0).abs() < 1e-5, "нормаль вдоль оси: {n}");
        }
        assert_eq!(p.triangles().count(), 12);
    }

    #[test]
    fn places_are_numbered_by_role() {
        let scene = Scene {
            regions: vec![
                region(Role::After, Vec3::ZERO),
                region(Role::Added, Vec3::X * 20.0),
                Region {
                    role: Role::Removed,
                    volume_mm3: 0.0,
                    mesh: Mesh::default(),
                },
                region(Role::Removed, Vec3::Y * 20.0),
                region(Role::Added, Vec3::Z * 20.0),
            ],
        };
        let parts = prepare(&scene);
        let got: Vec<_> = parts.iter().map(|p| (p.role, p.number)).collect();
        assert_eq!(
            got,
            [
                (Role::After, 0),
                (Role::Added, 1),
                (Role::Removed, 1),
                (Role::Added, 2)
            ],
            "пустое место пропущено"
        );
        assert_eq!(parts[1].bounds.min, [20.0, 0.0, 0.0]);
    }
}
