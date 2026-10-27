//! Геометрия на настоящем OpenCascade. Без него файл не компилируется
//! (тестов нет), а `available()` проверяется в `without_occt`.

#![cfg(nonius_occt)]

use std::f64::consts::PI;
use std::path::Path;

use nonius_core::compare::{ChangeKind, CompareContext};
use nonius_core::contract;
use nonius_core::extension::{Comparator, Exporter, GeometrySource, Renderer, Snapshot};
use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::PartName;
use nonius_occt::render::View;
use nonius_occt::{
    GeometryComparator, GeometryDiff, HighlightRenderer, OcctStepSource, PreviewExporter, Shape,
    CHANGES_PNG, MODEL_STEP, PREVIEW_PNG,
};

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// Пластина 60×30×8.
fn plate() -> Shape {
    Shape::make_box([0.0; 3], [60.0, 30.0, 8.0]).unwrap()
}

/// Пластина с отверстием ⌀8 насквозь и бобышкой 12×14×12 сверху.
fn plate_v2() -> Shape {
    let hole = Shape::make_cylinder([15.0, 15.0, -1.0], 4.0, 10.0).unwrap();
    let boss = Shape::make_box([40.0, 8.0, 8.0], [12.0, 14.0, 12.0]).unwrap();
    plate().cut(&hole, 0.0).unwrap().fuse(&boss, 0.0).unwrap()
}

fn snap(dir: &Path) -> Snapshot {
    Snapshot {
        item: PartName::new("пластина").unwrap(),
        version: None,
        dir: dir.to_path_buf(),
        meta: PartMeta::new(ItemKind::Part),
        tree: None,
    }
}

#[test]
fn box_properties() {
    let p = Shape::make_box([0.0; 3], [10.0, 20.0, 30.0])
        .unwrap()
        .props();
    assert!(close(p.volume, 6000.0, 1e-6), "{p:?}");
    assert!(close(p.area, 2200.0, 1e-6), "{p:?}");
    assert!(close(p.center[2], 15.0, 1e-9), "{p:?}");
    let (lo, hi) = p.bbox.unwrap();
    assert!(close(lo[0], 0.0, 1e-6) && close(hi[2], 30.0, 1e-6), "{p:?}");
    assert_eq!((p.solids, p.faces), (1, 6));
}

#[test]
fn step_round_trip_and_source_contract() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("пластина.step");
    plate_v2().write_step(&path).unwrap();

    let back = Shape::read_step(&path).unwrap().props();
    let original = plate_v2().props();
    assert!(
        close(back.volume, original.volume, 1e-3),
        "{back:?} vs {original:?}"
    );

    let src = OcctStepSource::new();
    if let Err(v) = contract::geometry_source(&src, &path) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
    let meta = src.read(&path).unwrap();
    let expected = 60.0 * 30.0 * 8.0 - PI * 16.0 * 8.0 + 12.0 * 14.0 * 12.0;
    assert!(
        close(meta.geometry.volume_mm3.unwrap(), expected, 0.5),
        "{meta:?}"
    );
    assert_eq!(meta.geometry.bodies, Some(1));
    assert!(meta.source.cad.is_some());
    assert!(Shape::read_step(&dir.path().join("нет.step")).is_err());
}

#[test]
fn diff_finds_added_and_removed_material() {
    let d = GeometryDiff::of_shapes(plate(), plate_v2(), 0.01).unwrap();
    assert!(d.changed());
    assert!(
        close(d.added_volume, 12.0 * 14.0 * 12.0, 0.5),
        "добавлено {}",
        d.added_volume
    );
    assert!(
        close(d.removed_volume, PI * 16.0 * 8.0, 0.5),
        "убрано {}",
        d.removed_volume
    );
    assert_eq!((d.added_pieces, d.removed_pieces), (1, 1));

    let same = GeometryDiff::of_shapes(plate(), plate(), 0.01).unwrap();
    assert!(
        !same.changed(),
        "{} / {}",
        same.added_volume,
        same.removed_volume
    );
}

#[test]
fn render_shows_green_boss_and_red_hole() {
    let d = GeometryDiff::of_shapes(plate(), plate_v2(), 0.01).unwrap();
    let img = d
        .render(&View {
            width: 320,
            height: 240,
            ..View::default()
        })
        .unwrap();
    let greenish = img
        .rgba
        .chunks_exact(4)
        .filter(|p| p[3] > 200 && p[1] as i32 - p[0] as i32 > 50)
        .count();
    let reddish = img
        .rgba
        .chunks_exact(4)
        .filter(|p| p[3] > 200 && p[0] as i32 - p[1] as i32 > 50)
        .count();
    assert!(greenish > 300, "зелёного мало: {greenish}");
    assert!(reddish > 50, "красного мало: {reddish}");
}

#[test]
fn comparator_and_renderer_modules() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    plate().write_step(&a.join(MODEL_STEP)).unwrap();
    plate_v2().write_step(&b.join(MODEL_STEP)).unwrap();
    let (sa, sb) = (snap(&a), snap(&b));

    let cmp = GeometryComparator::new();
    if let Err(v) = contract::comparator(&cmp, &sa, &sb) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
    let report = cmp.compare(&sa, &sb, &CompareContext::default()).unwrap();
    let kinds: Vec<_> = report.changes.iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        [ChangeKind::Added, ChangeKind::Removed],
        "{report:?}"
    );
    assert!(report.changes[0]
        .to_string()
        .starts_with("Добавлен материал"));

    let out = dir.path().join("картинки");
    let files = HighlightRenderer::new()
        .render(&sa, &sb, &report, &out)
        .unwrap();
    assert_eq!(files, [out.join(CHANGES_PNG)]);
    let png = std::fs::read(&files[0]).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

    // нет model.step — модули молча пропускают деталь
    let empty = snap(dir.path());
    assert!(cmp
        .compare(&empty, &sb, &CompareContext::default())
        .unwrap()
        .is_empty());
}

#[test]
fn preview_png_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let step = dir.path().join(MODEL_STEP);
    plate_v2().write_step(&step).unwrap();
    let png = dir.path().join("preview.png");
    assert!(nonius_occt::preview_png(&step, &png).unwrap());
    assert!(std::fs::metadata(&png).unwrap().len() > 1000);
    assert!(nonius_occt::occt_version().is_some());
}

#[test]
fn preview_exporter_contract() {
    let dir = tempfile::tempdir().unwrap();
    plate().write_step(&dir.path().join(MODEL_STEP)).unwrap();
    let out = dir.path().join("выгрузка");
    let exp = PreviewExporter::new();
    if let Err(v) = contract::exporter(&exp, &snap(dir.path()), &out) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
    let png = exp.export(&snap(dir.path()), "png", &out).unwrap();
    assert_eq!(png, out.join(PREVIEW_PNG));
    assert!(exp.export(&snap(dir.path()), "dxf", &out).is_err());
}
