//! Изменения словами на настоящих телах: грани сопоставляет OpenCascade,
//! слова складывает `nonius_occt::changes`.

#![cfg(nonius_occt)]

use std::path::Path;

use nonius_core::compare::CompareContext;
use nonius_core::contract;
use nonius_core::extension::{Comparator, Snapshot};
use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::PartName;
use nonius_occt::{changes, FeatureComparator, Shape, MODEL_STEP};

/// Пластина 60×30×`thick` с отверстием `diameter` насквозь в точке (x, 15).
fn plate(x: f64, diameter: f64, thick: f64) -> Shape {
    let plate = Shape::make_box([0.0; 3], [60.0, 30.0, thick]).unwrap();
    let hole = Shape::make_cylinder([x, 15.0, -1.0], diameter / 2.0, thick + 2.0).unwrap();
    plate.cut(&hole, 0.0).unwrap()
}

/// Записать в STEP и прочитать: у тела свои объекты граней, как у версий из CAD.
fn via_step(shape: &Shape, dir: &Path, name: &str) -> Shape {
    let path = dir.join(name);
    shape.write_step(&path).unwrap();
    Shape::read_step(&path).unwrap()
}

fn words(before: &Shape, after: &Shape) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let a = via_step(before, dir.path(), "v1.step");
    let b = via_step(after, dir.path(), "v2.step");
    let changed = a.changed_faces(&b).expect("версии сопоставляются");
    changes::describe(&changed.faces, 0.01)
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn hole_changes_are_said_in_words() {
    let v1 = plate(15.0, 8.0, 8.0);
    assert_eq!(words(&v1, &plate(15.0, 10.0, 8.0)), ["Отверстие: ⌀8 → ⌀10"]);
    assert_eq!(
        words(&v1, &plate(40.0, 8.0, 8.0)),
        ["Отверстие ⌀8 сдвинуто на 25 мм"]
    );
    assert_eq!(words(&v1, &v1), [] as [&str; 0]);
}

#[test]
fn new_hole_and_thicker_plate() {
    let v1 = plate(15.0, 8.0, 8.0);
    let second = Shape::make_cylinder([45.0, 15.0, -1.0], 2.5, 10.0).unwrap();
    assert_eq!(
        words(&v1, &v1.cut(&second, 0.0).unwrap()),
        ["Новое отверстие ⌀5"]
    );
    // Пластина толще на 0,5 мм: верх уехал наружу, отверстие стало глубже.
    assert_eq!(
        words(&v1, &plate(15.0, 8.0, 8.5)),
        [
            "Отверстие ⌀8, глубина: 8 мм → 8.5 мм",
            "Плоская грань сдвинута наружу на 0.5 мм"
        ]
    );
}

#[test]
fn changed_faces_are_only_the_changed_ones() {
    let dir = tempfile::tempdir().unwrap();
    let a = via_step(&plate(15.0, 8.0, 8.0), dir.path(), "v1.step");
    let b = via_step(&plate(40.0, 8.0, 8.0), dir.path(), "v2.step");
    let changed = a.changed_faces(&b).unwrap();
    // Стенка отверстия и две грани пластины, в которых оно прорезано; четыре
    // боковые грани совпали.
    assert!(changed.faces.len() <= 8, "{}", changed.faces.len());
    assert!(!changed.fresh.mesh(0.1, 0.1).triangles.is_empty());
    assert!(!changed.gone.mesh(0.1, 0.1).triangles.is_empty());
}

#[test]
fn feature_comparator_module() {
    let dir = tempfile::tempdir().unwrap();
    let snap = |name: &str, shape: &Shape| {
        let d = dir.path().join(name);
        std::fs::create_dir_all(&d).unwrap();
        shape.write_step(&d.join(MODEL_STEP)).unwrap();
        Snapshot {
            item: PartName::new("пластина").unwrap(),
            version: None,
            dir: d,
            meta: PartMeta::new(ItemKind::Part),
            tree: None,
        }
    };
    let a = snap("a", &plate(15.0, 8.0, 8.0));
    let b = snap("b", &plate(15.0, 10.0, 8.0));
    let cmp = FeatureComparator::new();
    if let Err(v) = contract::comparator(&cmp, &a, &b) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
    let report = cmp.compare(&a, &b, &CompareContext::default()).unwrap();
    let lines: Vec<String> = report.changes.iter().map(ToString::to_string).collect();
    assert_eq!(lines, ["Отверстие: ⌀8 → ⌀10"]);
}
