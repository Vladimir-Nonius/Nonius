//! Плоские грани: сдвинутые плоскости.
//!
//! Грань на той же плоскости, что и раньше, — не изменение: у неё другие
//! края (в ней прорезали отверстие), и про это скажут другие строки. Грань,
//! параллельная пропавшей, рядом с ней и похожая по размеру — сдвинутая
//! плоскость: «наружу» — материала стало больше, «внутрь» — меньше.

use nonius_core::compare::Change;

use super::text::{dot, length, mm, same_direction, scaled, sub};
use super::{line, Face, Side};

struct Flat {
    face: Face,
    /// Расстояние плоскости от начала координат вдоль нормали.
    offset: f64,
    used: bool,
}

/// На сколько «стало» дальше «было» вдоль нормали; `None` — не параллельны.
fn shift(before: &Flat, after: &Flat) -> Option<f64> {
    same_direction(before.face.dir, after.face.dir).then_some(after.offset - before.offset)
}

/// Грани друг над другом и похожи по размеру.
fn alike(before: &Face, after: &Face) -> bool {
    let between = sub(after.center, before.center);
    let along = dot(between, before.dir);
    let sideways = length(sub(between, scaled(before.dir, along)));
    let (small, large) = if before.area < after.area {
        (before.area, after.area)
    } else {
        (after.area, before.area)
    };
    small > 0.0 && large <= 2.0 * small && sideways <= large.sqrt()
}

/// Описать плоские грани. Возвращает грани, которым слов не нашлось.
pub(super) fn describe(faces: &[Face], tol: f64, out: &mut Vec<Change>) -> Vec<Face> {
    let mut all: Vec<Flat> = faces
        .iter()
        .map(|f| Flat {
            face: *f,
            offset: dot(f.origin, f.dir),
            used: false,
        })
        .collect();
    let sides = |all: &[Flat]| -> (Vec<usize>, Vec<usize>) {
        (0..all.len()).partition(|&i| all[i].face.side == Side::Before)
    };
    let (before, after) = sides(&all);

    // Та же плоскость: изменились только края грани.
    for &i in &before {
        for &j in &after {
            if shift(&all[i], &all[j]).is_some_and(|d| d.abs() <= tol) {
                all[i].used = true;
                all[j].used = true;
            }
        }
    }

    // Сдвинутые плоскости: ближайшие пары первыми.
    let mut found: Vec<(f64, usize, usize)> = Vec::new();
    for &i in before.iter().filter(|&&i| !all[i].used) {
        for &j in after.iter().filter(|&&j| !all[j].used) {
            if let Some(d) = shift(&all[i], &all[j]) {
                if alike(&all[i].face, &all[j].face) {
                    found.push((d, i, j));
                }
            }
        }
    }
    found.sort_by(|x, y| x.0.abs().total_cmp(&y.0.abs()));
    for (d, i, j) in found {
        if all[i].used || all[j].used {
            continue;
        }
        all[i].used = true;
        all[j].used = true;
        let way = if d > 0.0 {
            "наружу"
        } else {
            "внутрь"
        };
        out.push(line(
            format!("Плоская грань сдвинута {way} на {} мм", mm(d.abs())),
            None,
            None,
        ));
    }
    all.into_iter()
        .filter(|f| !f.used)
        .map(|f| f.face)
        .collect()
}
