//! Честность сравнения формы: мелкое изменение в большой детали не теряется,
//! а файл без твёрдых тел не выдаётся за «изменений нет».

#![cfg(nonius_occt)]

use std::f64::consts::PI;

use nonius_core::compare::CompareContext;
use nonius_core::extension::{Comparator, Snapshot};
use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::PartName;
use nonius_occt::{GeometryComparator, GeometryDiff, Shape, MODEL_STEP};

/// Куб со стороной метр: миллиард мм³.
fn big_block() -> Shape {
    Shape::make_box([0.0; 3], [1000.0, 1000.0, 1000.0]).unwrap()
}

#[test]
fn small_hole_in_a_big_part_is_seen() {
    // Отверстие ⌀6 глубиной 20 мм — 565 мм³, полмиллионной доли детали.
    // Порог «доля объёма детали» его прятал.
    let hole = Shape::make_cylinder([500.0, 500.0, 980.0], 3.0, 30.0).unwrap();
    let drilled = big_block().cut(&hole, 0.0).unwrap();
    let d = GeometryDiff::of_shapes(big_block(), drilled, 0.01).unwrap();
    let expected = PI * 9.0 * 20.0;
    assert!(d.has_removed() && !d.has_added());
    assert!(
        (d.removed_volume - expected).abs() < 0.5,
        "убрано {}",
        d.removed_volume
    );
    assert_eq!(d.removed_pieces, 1);
    assert_eq!(d.unsure, None);
    // Место попадает и в сцену для окна.
    assert_eq!(d.scene().regions.len(), 3, "стало, было и одно место");
}

#[test]
fn layer_thinner_than_tolerance_is_noise() {
    // Грань сдвинута на 0,005 мм при допуске 0,01: это не изменение.
    let grown = Shape::make_box([0.0; 3], [1000.0, 1000.0, 1000.005]).unwrap();
    let d = GeometryDiff::of_shapes(big_block(), grown, 0.01).unwrap();
    assert!(!d.changed(), "{} / {}", d.added_volume, d.removed_volume);
    // При допуске мельче тот же слой — уже изменение.
    let grown = Shape::make_box([0.0; 3], [1000.0, 1000.0, 1000.005]).unwrap();
    let d = GeometryDiff::of_shapes(big_block(), grown, 0.001).unwrap();
    assert!(d.has_added(), "добавлено {}", d.added_volume);
}

/// STEP куба, в котором твёрдое тело заменено оболочкой из тех же граней:
/// так выглядят файлы, выгруженные «поверхностями».
fn surfaces_only(dir: &std::path::Path) -> std::path::PathBuf {
    let solid = dir.join("solid.step");
    Shape::make_box([0.0; 3], [10.0, 10.0, 10.0])
        .unwrap()
        .write_step(&solid)
        .unwrap();
    let text = std::fs::read_to_string(&solid).unwrap();
    let start = text.find("MANIFOLD_SOLID_BREP(").expect("в STEP есть тело");
    let end = start + text[start..].find(");").unwrap();
    let args = &text[start + "MANIFOLD_SOLID_BREP(".len()..end];
    let (name, shell) = args.rsplit_once(',').unwrap();
    let surface = format!("SHELL_BASED_SURFACE_MODEL({name},({}))", shell.trim());
    let text = format!("{}{}{}", &text[..start], surface, &text[end + 1..]).replace(
        "ADVANCED_BREP_SHAPE_REPRESENTATION",
        "MANIFOLD_SURFACE_SHAPE_REPRESENTATION",
    );
    let path = dir.join("surfaces.step");
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn file_without_solids_is_said_aloud() {
    let dir = tempfile::tempdir().unwrap();
    let surfaces = Shape::read_step(&surfaces_only(dir.path())).unwrap();
    assert_eq!(surfaces.solid_count(), 0, "в файле только поверхности");
    let solid = Shape::make_box([0.0; 3], [10.0, 10.0, 12.0]).unwrap();
    let d = GeometryDiff::of_shapes(surfaces, solid, 0.01).unwrap();
    assert!(d.no_solids && !d.changed());

    // В отчёте сравнения это строка словами, а не пустой отчёт.
    let snap = |name: &str, shape: &std::path::Path| {
        let folder = dir.path().join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::copy(shape, folder.join(MODEL_STEP)).unwrap();
        Snapshot {
            item: PartName::new("куб").unwrap(),
            version: None,
            dir: folder,
            meta: PartMeta::new(ItemKind::Part),
            tree: None,
        }
    };
    let a = snap("было", &dir.path().join("surfaces.step"));
    let b = snap("стало", &dir.path().join("solid.step"));
    let report = GeometryComparator::new()
        .compare(&a, &b, &CompareContext::default())
        .unwrap();
    let said: Vec<String> = report.changes.iter().map(|c| c.to_string()).collect();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("нет твёрдых тел"), "{said:?}");
}
