//! Что под курсором: луч из камеры против треугольников видимых кусков.

use glam::Vec3;

use super::mesh::Part;

/// Ближайший кусок на луче `origin + t·dir` (t может быть и отрицательным:
/// у ортогональной камеры луч идёт сквозь всю сцену). Возвращает номер куска
/// в `parts`. `visible` — какие куски сейчас показаны.
pub fn pick(parts: &[Part], visible: &[bool], origin: Vec3, dir: Vec3) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, part) in parts.iter().enumerate() {
        if !visible.get(i).copied().unwrap_or(false) || !hits_box(part, origin, dir) {
            continue;
        }
        for tri in part.triangles() {
            if let Some(t) = hit(tri, origin, dir) {
                if best.is_none_or(|(_, b)| t < b) {
                    best = Some((i, t));
                }
            }
        }
    }
    best.map(|(i, _)| i)
}

/// Пересекает ли луч габариты куска (быстрый отсев).
fn hits_box(part: &Part, origin: Vec3, dir: Vec3) -> bool {
    let (lo, hi) = (Vec3::from(part.bounds.min), Vec3::from(part.bounds.max));
    let pad = (hi - lo).max_element() * 1e-3 + 1e-4;
    let (lo, hi) = (lo - pad, hi + pad);
    let (mut t0, mut t1) = (f32::NEG_INFINITY, f32::INFINITY);
    for k in 0..3 {
        if dir[k].abs() < 1e-12 {
            if origin[k] < lo[k] || origin[k] > hi[k] {
                return false;
            }
            continue;
        }
        let (a, b) = ((lo[k] - origin[k]) / dir[k], (hi[k] - origin[k]) / dir[k]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    t0 <= t1
}

/// Пересечение луча с треугольником (Мёллер — Трумбор), с обеих сторон.
fn hit([a, b, c]: [Vec3; 3], origin: Vec3, dir: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(e2.dot(q) * inv)
}

#[cfg(test)]
mod tests {
    use super::super::mesh::tests::cube_mesh;
    use super::*;
    use nonius_scene::{Region, Role};

    fn part(role: Role, at: Vec3) -> Part {
        let r = Region {
            role,
            volume_mm3: 1.0,
            mesh: cube_mesh(at, 10.0),
        };
        Part::new(&r, 1).unwrap()
    }

    #[test]
    fn nearest_visible_part_wins() {
        // Два куба друг за другом вдоль Z; смотрим спереди (вдоль −Z).
        let parts = [
            part(Role::Added, Vec3::ZERO),
            part(Role::Removed, Vec3::Z * 20.0),
        ];
        let (origin, dir) = (Vec3::new(5.0, 5.0, 0.0), Vec3::NEG_Z);
        assert_eq!(pick(&parts, &[true, true], origin, dir), Some(1));
        assert_eq!(pick(&parts, &[true, false], origin, dir), Some(0));
        assert_eq!(pick(&parts, &[false, false], origin, dir), None);
        // Мимо.
        let miss = Vec3::new(50.0, 5.0, 0.0);
        assert_eq!(pick(&parts, &[true, true], miss, dir), None);
    }
}
