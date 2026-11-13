//! Тесты описания изменений словами: грани заданы числами, без OpenCascade.

use std::f64::consts::PI;

use super::*;

/// Половинка вертикального цилиндра в точке (x, y), от z = 0 до `depth`.
fn half(side: Side, x: f64, y: f64, radius: f64, depth: f64, inward: bool) -> Face {
    Face {
        side,
        surface: Surface::Cylinder,
        inward,
        origin: [x, y, 0.0],
        dir: [0.0, 0.0, 1.0],
        radius,
        radius2: 0.0,
        angle: PI,
        span: (0.0, depth),
        area: PI * radius * depth,
        center: [x, y, depth / 2.0],
    }
}

/// Отверстие из двух половинок, как его выгружает SolidWorks.
fn hole(side: Side, x: f64, y: f64, diameter: f64, depth: f64) -> [Face; 2] {
    [half(side, x, y, diameter / 2.0, depth, true); 2]
}

/// Горизонтальная грань 20×20 на высоте `z`, нормаль вверх.
fn top(side: Side, z: f64) -> Face {
    Face {
        side,
        surface: Surface::Plane,
        inward: false,
        origin: [0.0, 0.0, z],
        dir: [0.0, 0.0, 1.0],
        radius: 0.0,
        radius2: 0.0,
        angle: 0.0,
        span: (0.0, 0.0),
        area: 400.0,
        center: [10.0, 10.0, z],
    }
}

fn words(faces: &[Face]) -> Vec<String> {
    describe(faces, 0.01)
        .iter()
        .map(Change::to_string)
        .collect()
}

#[test]
fn hole_got_bigger() {
    let mut f = hole(Side::Before, 10.0, 10.0, 8.0, 5.0).to_vec();
    f.extend(hole(Side::After, 10.0, 10.0, 10.0, 5.0));
    assert_eq!(words(&f), ["Отверстие: ⌀8 → ⌀10"]);
}

#[test]
fn hole_got_deeper() {
    let mut f = hole(Side::Before, 10.0, 10.0, 8.0, 5.0).to_vec();
    f.extend(hole(Side::After, 10.0, 10.0, 8.0, 7.5));
    assert_eq!(words(&f), ["Отверстие ⌀8, глубина: 5 мм → 7.5 мм"]);
}

#[test]
fn hole_moved() {
    let mut f = hole(Side::Before, 10.0, 10.0, 8.0, 5.0).to_vec();
    f.extend(hole(Side::After, 13.0, 14.0, 8.0, 5.0));
    assert_eq!(words(&f), ["Отверстие ⌀8 сдвинуто на 5 мм"]);
}

#[test]
fn new_holes_are_counted() {
    let mut f = Vec::new();
    for x in [10.0, 20.0, 30.0, 40.0] {
        f.extend(hole(Side::After, x, 0.0, 5.0, 3.0));
    }
    f.extend(hole(Side::Before, 0.0, 50.0, 6.0, 3.0));
    assert_eq!(
        words(&f),
        ["Новое отверстие ⌀5, 4 шт.", "Убрано отверстие ⌀6"]
    );
}

#[test]
fn reversed_axis_is_the_same_axis() {
    let mut f = hole(Side::Before, 10.0, 10.0, 8.0, 5.0).to_vec();
    let mut flipped = hole(Side::After, 10.0, 10.0, 9.0, 5.0);
    for h in &mut flipped {
        h.origin = [10.0, 10.0, 5.0];
        h.dir = [0.0, 0.0, -1.0];
    }
    f.extend(flipped);
    assert_eq!(words(&f), ["Отверстие: ⌀8 → ⌀9"]);
}

#[test]
fn boss_is_not_a_hole() {
    let f = [half(Side::After, 0.0, 0.0, 10.0, 12.0, false); 2];
    assert_eq!(words(&f), ["Новая бобышка ⌀20"]);
}

#[test]
fn fillets_changed_radius() {
    let quarter = |side, x: f64, radius: f64| Face {
        angle: PI / 2.0,
        ..half(side, x, 0.0, radius, 30.0, false)
    };
    let f = [
        quarter(Side::Before, 0.0, 3.0),
        quarter(Side::Before, 50.0, 3.0),
        quarter(Side::After, 2.0, 5.0),
        quarter(Side::After, 48.0, 5.0),
    ];
    assert_eq!(words(&f), ["Скругление, 2 шт.: R3 → R5"]);

    // Скругление на месте, изменились только его края — молчим.
    let f = [
        quarter(Side::Before, 0.0, 3.0),
        Face {
            span: (0.0, 20.0),
            ..quarter(Side::After, 0.0, 3.0)
        },
    ];
    assert_eq!(words(&f), [] as [&str; 0]);
}

#[test]
fn plane_moved_out() {
    let f = [top(Side::Before, 8.0), top(Side::After, 8.5)];
    assert_eq!(words(&f), ["Плоская грань сдвинута наружу на 0.5 мм"]);
    let f = [top(Side::Before, 8.0), top(Side::After, 6.0)];
    assert_eq!(words(&f), ["Плоская грань сдвинута внутрь на 2 мм"]);
}

#[test]
fn same_plane_with_new_edges_is_silent() {
    // В верхней грани прорезали отверстие: грань та же, края другие.
    let mut f = vec![top(Side::Before, 8.0), top(Side::After, 8.0)];
    f.extend(hole(Side::After, 10.0, 10.0, 8.0, 8.0));
    assert_eq!(words(&f), ["Новое отверстие ⌀8"]);
}

#[test]
fn unexplained_faces_are_counted() {
    let spline = Face {
        surface: Surface::Other,
        ..top(Side::After, 0.0)
    };
    let half_only = half(Side::Before, 0.0, 0.0, 4.0, 5.0, true);
    assert_eq!(
        words(&[spline, spline, half_only]),
        ["Другие изменённые грани: 1 → 2"]
    );
    assert_eq!(words(&[]), [] as [&str; 0]);
}
