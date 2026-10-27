//! Объёмная модель изменений `changes.glb` на настоящем OpenCascade.

#![cfg(nonius_occt)]

use std::path::Path;

use nonius_core::compare::ChangeReport;
use nonius_core::contract;
use nonius_core::extension::{ExtensionPoint, Registry, Renderer, Snapshot};
use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::PartName;
use nonius_occt::glb::parse_glb;
use nonius_occt::{
    shape_glb, GeometryDiff, SceneRenderer, Shape, CHANGES_GLB, MODEL_STEP, SCENE_RENDERER,
};
use serde_json::Value;

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

fn roles(doc: &Value) -> Vec<String> {
    doc["nodes"]
        .as_array()
        .map(|nodes| {
            nodes
                .iter()
                .map(|n| n["extras"]["role"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn diff_scene_has_three_colored_layers() {
    let diff = GeometryDiff::of_shapes(plate(), plate_v2(), 0.01).unwrap();
    let (doc, bin) = parse_glb(&diff.to_glb()).expect("корректный GLB");
    assert_eq!(roles(&doc), ["unchanged", "added", "removed"]);
    assert!(!bin.is_empty());

    // Бобышка 12×14×12 = 2016 мм³, отверстие π·4²·8 ≈ 402 мм³.
    let added = doc["nodes"][1]["extras"]["volume_mm3"].as_f64().unwrap();
    let removed = doc["nodes"][2]["extras"]["volume_mm3"].as_f64().unwrap();
    assert!((added - 2016.0).abs() < 1.0, "{added}");
    assert!((removed - 402.1).abs() < 1.0, "{removed}");

    // Серое полупрозрачное, цветное — нет.
    assert_eq!(doc["materials"][0]["alphaMode"], "BLEND");
    assert!(doc["materials"][1].get("alphaMode").is_none());

    // Габариты новой версии в метрах, Y вверх: высота 8 + 12 = 20 мм.
    let max_y = doc["accessors"][0]["max"][1].as_f64().unwrap();
    assert!((max_y - 0.020).abs() < 1e-5, "{max_y}");
}

#[test]
fn same_part_is_one_opaque_layer() {
    let (doc, _) = parse_glb(&shape_glb(&plate())).unwrap();
    assert_eq!(roles(&doc), ["unchanged"]);
    assert!(doc["materials"][0].get("alphaMode").is_none());

    let diff = GeometryDiff::of_shapes(plate(), plate(), 0.01).unwrap();
    let (doc, _) = parse_glb(&diff.to_glb()).unwrap();
    assert_eq!(roles(&doc), ["unchanged"]);
}

#[test]
fn scene_renderer_module() {
    let renderer = SceneRenderer::new();
    if let Err(v) = contract::manifest(&renderer, ExtensionPoint::Renderer) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
    assert_eq!(SceneRenderer::ID, SCENE_RENDERER);

    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    plate().write_step(&a.join(MODEL_STEP)).unwrap();
    plate_v2().write_step(&b.join(MODEL_STEP)).unwrap();

    let out = dir.path().join("модель");
    let files = renderer
        .render(&snap(&a), &snap(&b), &ChangeReport::empty(""), &out)
        .unwrap();
    assert_eq!(files, [out.join(CHANGES_GLB)]);
    let (doc, _) = parse_glb(&std::fs::read(&files[0]).unwrap()).unwrap();
    assert_eq!(roles(&doc), ["unchanged", "added", "removed"]);

    // Одна и та же версия — деталь целиком, без булевых операций.
    let files = renderer
        .render(&snap(&b), &snap(&b), &ChangeReport::empty(""), &out)
        .unwrap();
    let (doc, _) = parse_glb(&std::fs::read(&files[0]).unwrap()).unwrap();
    assert_eq!(roles(&doc), ["unchanged"]);

    // Нет model.step — модуль молча пропускает деталь.
    assert!(renderer
        .render(&snap(dir.path()), &snap(&b), &ChangeReport::empty(""), &out)
        .unwrap()
        .is_empty());
}

#[test]
fn registered_after_png_renderer() {
    let mut reg = Registry::with_builtins();
    nonius_occt::register(&mut reg).unwrap();
    let ids: Vec<_> = reg.renderers().map(|r| r.manifest().id.clone()).collect();
    assert_eq!(ids, ["highlight-occt", SCENE_RENDERER]);
}
