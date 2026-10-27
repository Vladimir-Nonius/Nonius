//! Сравнение версий, которые почти целиком совпадают: одинаковые грани
//! «было» и «стало» делаются общими (`cpp/share.cpp`), а прочитанные STEP
//! берутся из кэша. Результат должен быть тем же, что у честного пересечения.

#![cfg(nonius_occt)]

use std::f64::consts::PI;
use std::path::Path;

use nonius_occt::{GeometryDiff, Shape};

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// Пластина 60×30×8 с отверстием ⌀8 насквозь в точке (x, 15).
fn plate_with_hole(x: f64) -> Shape {
    let plate = Shape::make_box([0.0; 3], [60.0, 30.0, 8.0]).unwrap();
    let hole = Shape::make_cylinder([x, 15.0, -1.0], 4.0, 10.0).unwrap();
    plate.cut(&hole, 0.0).unwrap()
}

/// Записать в STEP и прочитать: у тела свои объекты граней, как у версий из CAD.
fn via_step(shape: &Shape, dir: &Path, name: &str) -> Shape {
    let path = dir.join(name);
    shape.write_step(&path).unwrap();
    Shape::read_step(&path).unwrap()
}

const HOLE: f64 = PI * 16.0 * 8.0;

#[test]
fn moved_hole_is_both_added_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let a = via_step(&plate_with_hole(15.0), dir.path(), "v1.step");
    let b = via_step(&plate_with_hole(40.0), dir.path(), "v2.step");
    let d = GeometryDiff::of_shapes(a, b, 0.01).unwrap();
    assert!(
        close(d.added_volume, HOLE, 0.5),
        "добавлено {}",
        d.added_volume
    );
    assert!(
        close(d.removed_volume, HOLE, 0.5),
        "убрано {}",
        d.removed_volume
    );
    assert_eq!((d.added_pieces, d.removed_pieces), (1, 1));
}

#[test]
fn pocket_in_top_face_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let pocket = Shape::make_box([30.0, 5.0, 5.0], [20.0, 10.0, 4.0]).unwrap();
    let v2 = plate_with_hole(15.0).cut(&pocket, 0.0).unwrap();
    let a = via_step(&plate_with_hole(15.0), dir.path(), "v1.step");
    let b = via_step(&v2, dir.path(), "v2.step");
    let d = GeometryDiff::of_shapes(a, b, 0.01).unwrap();
    assert!(!d.has_added(), "добавлено {}", d.added_volume);
    assert!(
        close(d.removed_volume, 20.0 * 10.0 * 3.0, 0.5),
        "убрано {}",
        d.removed_volume
    );
    assert_eq!(d.removed_pieces, 1);
}

#[test]
fn same_part_from_two_files_has_no_changes() {
    let dir = tempfile::tempdir().unwrap();
    let a = via_step(&plate_with_hole(15.0), dir.path(), "v1.step");
    let b = via_step(&plate_with_hole(15.0), dir.path(), "v2.step");
    let d = GeometryDiff::of_shapes(a, b, 0.01).unwrap();
    assert!(!d.changed(), "{} / {}", d.added_volume, d.removed_volume);
}

#[test]
fn nothing_in_common_still_compares() {
    let a = Shape::make_box([0.0; 3], [10.0, 10.0, 10.0]).unwrap();
    let b = Shape::make_cylinder([5.0, 5.0, 0.0], 5.0, 20.0).unwrap();
    let d = GeometryDiff::of_shapes(a, b, 0.01).unwrap();
    let common = PI * 25.0 * 10.0;
    assert!(
        close(d.removed_volume, 1000.0 - common, 0.5),
        "{}",
        d.removed_volume
    );
    assert!(
        close(d.added_volume, PI * 25.0 * 20.0 - common, 0.5),
        "{}",
        d.added_volume
    );
}

#[test]
fn cached_step_follows_file_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("деталь.step");
    plate_with_hole(15.0).write_step(&path).unwrap();
    let first = Shape::read_step(&path).unwrap().props();
    let again = Shape::read_step(&path).unwrap().props();
    assert_eq!(first, again);

    // Тот же путь, другое содержимое — кэш не должен подсунуть старое тело.
    Shape::make_box([0.0; 3], [5.0, 5.0, 5.0])
        .unwrap()
        .write_step(&path)
        .unwrap();
    let changed = Shape::read_step(&path).unwrap().props();
    assert!(close(changed.volume, 125.0, 1e-6), "{changed:?}");
}

#[test]
fn volume_and_rough_bbox_agree_with_props() {
    let s = plate_with_hole(15.0);
    let p = s.props();
    assert!(
        close(s.volume(), p.volume, 1e-9),
        "{} / {}",
        s.volume(),
        p.volume
    );
    assert_eq!(s.solid_count(), p.solids);
    let (lo, hi) = p.bbox.unwrap();
    let (rlo, rhi) = s.rough_bbox().unwrap();
    for k in 0..3 {
        assert!(
            rlo[k] <= lo[k] + 1e-6 && rhi[k] >= hi[k] - 1e-6,
            "{rlo:?} {rhi:?}"
        );
    }
}
