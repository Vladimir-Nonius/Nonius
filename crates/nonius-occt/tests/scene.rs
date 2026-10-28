//! Объёмная модель изменений `changes.glb` на настоящем OpenCascade.

#![cfg(nonius_occt)]

use std::path::Path;

use nonius_core::compare::ChangeReport;
use nonius_core::contract as core_contract;
use nonius_core::extension::{ExtensionPoint, Registry, Renderer, Snapshot};
use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::PartName;
use nonius_occt::{
    shape_scene, GeometryDiff, SceneRenderer, Shape, CHANGES_GLB, MODEL_STEP, SCENE_RENDERER,
};
use nonius_scene::{contract, Role, Scene};

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

fn roles(scene: &Scene) -> Vec<Role> {
    scene.regions.iter().map(|r| r.role).collect()
}

/// Пластина с двумя бобышками и двумя отверстиями: четыре места.
fn plate_v3() -> Shape {
    let mut s = plate();
    for x in [10.0, 25.0] {
        let hole = Shape::make_cylinder([x, 15.0, -1.0], 3.0, 10.0).unwrap();
        s = s.cut(&hole, 0.0).unwrap();
    }
    for x in [38.0, 50.0] {
        let boss = Shape::make_box([x, 5.0, 8.0], [6.0, 6.0, 5.0]).unwrap();
        s = s.fuse(&boss, 0.0).unwrap();
    }
    s
}

#[test]
fn diff_scene_has_places() {
    let diff = GeometryDiff::of_shapes(plate(), plate_v2(), 0.01).unwrap();
    let scene = diff.scene();
    contract::scene(&scene).unwrap_or_else(|v| panic!("{v:#?}"));
    // Крупное первым: бобышка 12×14×12 = 2016 мм³, отверстие π·4²·8 ≈ 402 мм³.
    assert_eq!(
        roles(&scene),
        [Role::After, Role::Before, Role::Added, Role::Removed]
    );
    assert!((scene.regions[1].volume_mm3 - 14400.0).abs() < 1.0);
    assert!((scene.regions[2].volume_mm3 - 2016.0).abs() < 1.0);
    assert!((scene.regions[3].volume_mm3 - 402.1).abs() < 1.0);
    // Габариты новой версии: высота 8 + 12 = 20 мм.
    let b = scene.bounds().unwrap();
    assert!((b.max[2] - 20.0).abs() < 1e-3, "{b:?}");
}

#[test]
fn each_piece_is_its_own_place() {
    let scene = GeometryDiff::of_shapes(plate(), plate_v3(), 0.01)
        .unwrap()
        .scene();
    assert_eq!(scene.regions_of(Role::Added).count(), 2);
    assert_eq!(scene.regions_of(Role::Removed).count(), 2);
    for r in scene.changes() {
        let b = r.mesh.bounds().unwrap();
        // каждое место — один кусок, а не весь слой
        assert!(b.max[0] - b.min[0] < 10.0, "{:?}: {b:?}", r.role);
    }
}

#[test]
fn same_part_is_one_place() {
    let scene = shape_scene(&plate());
    assert_eq!(roles(&scene), [Role::After]);
    assert!(!scene.has_changes());
    let scene = GeometryDiff::of_shapes(plate(), plate(), 0.01)
        .unwrap()
        .scene();
    assert_eq!(roles(&scene), [Role::After]);
}

#[test]
fn scene_renderer_module() {
    let renderer = SceneRenderer::new();
    if let Err(v) = core_contract::manifest(&renderer, ExtensionPoint::Renderer) {
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
    let scene =
        contract::glb(&std::fs::read(&files[0]).unwrap()).unwrap_or_else(|v| panic!("{v:#?}"));
    assert_eq!(
        roles(&scene),
        [Role::After, Role::Added, Role::Removed, Role::Before]
    );

    // Одна и та же версия — деталь целиком, без булевых операций.
    let files = renderer
        .render(&snap(&b), &snap(&b), &ChangeReport::empty(""), &out)
        .unwrap();
    let scene = contract::glb(&std::fs::read(&files[0]).unwrap()).unwrap();
    assert_eq!(roles(&scene), [Role::After]);

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
