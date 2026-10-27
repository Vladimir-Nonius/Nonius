//! Формат сцены: запись, чтение, контракт, совместимость со старыми файлами.

use nonius_scene::{contract, Bounds, Mesh, Region, Role, Scene, SceneError, FORMAT_VERSION};

/// Параллелепипед от угла `o` с размерами `s`, 12 треугольников.
fn cuboid(o: [f32; 3], s: [f32; 3]) -> Mesh {
    let mut positions = Vec::new();
    for i in 0..8u32 {
        positions.push([
            o[0] + if i & 1 != 0 { s[0] } else { 0.0 },
            o[1] + if i & 2 != 0 { s[1] } else { 0.0 },
            o[2] + if i & 4 != 0 { s[2] } else { 0.0 },
        ]);
    }
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let triangles = quads
        .iter()
        .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
        .collect();
    Mesh {
        positions,
        triangles,
    }
}

/// Пластина 60×30×8: стало — с бобышкой (добавлено), без выреза (убрано).
fn sample() -> Scene {
    Scene {
        regions: vec![
            Region {
                role: Role::After,
                volume_mm3: 15904.0,
                mesh: cuboid([0.0; 3], [60.0, 30.0, 8.0]),
            },
            Region {
                role: Role::Before,
                volume_mm3: 14400.0,
                mesh: cuboid([0.0; 3], [60.0, 30.0, 8.0]),
            },
            Region {
                role: Role::Added,
                volume_mm3: 2016.0,
                mesh: cuboid([40.0, 8.0, 8.0], [12.0, 14.0, 12.0]),
            },
            Region {
                role: Role::Removed,
                volume_mm3: 512.0,
                mesh: cuboid([10.0, 10.0, 0.0], [8.0, 8.0, 8.0]),
            },
        ],
    }
}

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3)
}

#[test]
fn round_trip_keeps_everything() {
    let scene = sample();
    let read = Scene::from_glb(&scene.to_glb()).unwrap();
    assert_eq!(read.regions.len(), 4);
    // Старая версия читается последней: она во второй сцене glTF.
    let mut expected = scene.regions.clone();
    expected.sort_by_key(|r| r.role == Role::Before);
    for (a, b) in expected.iter().zip(&read.regions) {
        assert_eq!(a.role, b.role);
        assert_eq!(a.volume_mm3, b.volume_mm3);
        assert_eq!(a.mesh.triangles, b.mesh.triangles);
        assert!(a
            .mesh
            .positions
            .iter()
            .zip(&b.mesh.positions)
            .all(|(p, q)| close(*p, *q)));
    }
    assert!(read.has_changes());
    assert_eq!(read.changes().count(), 2);
    assert_eq!(read.volume_of(Role::Added), 2016.0);
    let b = read.bounds().unwrap();
    assert!(
        close(b.min, [0.0; 3]) && close(b.max, [60.0, 30.0, 20.0]),
        "{b:?}"
    );
}

#[test]
fn written_scene_passes_contract() {
    let read = contract::glb(&sample().to_glb()).unwrap_or_else(|v| panic!("{v:#?}"));
    assert_eq!(read.regions.len(), 4);
}

#[test]
fn contract_catches_bad_scenes() {
    assert!(contract::scene(&Scene::default()).is_err());

    let mut bad = sample();
    bad.regions[2].mesh.triangles.push([0, 1, 99]);
    bad.regions[3].volume_mm3 = -1.0;
    let v = contract::scene(&bad).unwrap_err();
    assert_eq!(v.len(), 2, "{v:?}");

    // glTF без номера формата — не сцена Nonius.
    let foreign = br#"{"asset":{"version":"2.0"}}"#;
    assert!(contract::glb(&glb_of_json(foreign)).is_err());
}

/// Минимальный `.glb` только с JSON-частью.
fn glb_of_json(json: &[u8]) -> Vec<u8> {
    let mut j = json.to_vec();
    while j.len() % 4 != 0 {
        j.push(b' ');
    }
    let total = 12 + 8 + j.len();
    let mut out = Vec::new();
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&j);
    out
}

#[test]
fn newer_format_is_refused_with_explanation() {
    let json = format!(
        r#"{{"asset":{{"version":"2.0","extras":{{"nonius_scene":{}}}}}}}"#,
        FORMAT_VERSION + 1
    );
    let err = Scene::from_glb(&glb_of_json(json.as_bytes())).unwrap_err();
    assert_eq!(
        err,
        SceneError::TooNew {
            found: FORMAT_VERSION as u64 + 1
        }
    );
    assert!(err.to_string().contains("обновите программу"));
}

/// Сторонние просмотрщики показывают сцену по умолчанию: в ней нет старой версии.
#[test]
fn before_lives_in_second_gltf_scene() {
    let gltf = gltf::Gltf::from_slice(&sample().to_glb()).unwrap();
    let default: Vec<String> = gltf
        .default_scene()
        .unwrap()
        .nodes()
        .map(|n| n.name().unwrap().to_string())
        .collect();
    assert_eq!(default, ["Стало 1", "Добавлено 1", "Убрано 1"]);
    let second: Vec<_> = gltf.scenes().nth(1).unwrap().nodes().collect();
    assert_eq!(second.len(), 1);
}

#[test]
fn garbage_is_broken() {
    assert!(matches!(
        Scene::from_glb(b"not a glb"),
        Err(SceneError::Broken(_))
    ));
}

#[test]
fn empty_regions_are_skipped() {
    let mut scene = sample();
    scene.regions.push(Region {
        role: Role::Added,
        volume_mm3: 0.0,
        mesh: Mesh::default(),
    });
    assert_eq!(Scene::from_glb(&scene.to_glb()).unwrap().regions.len(), 4);
}

/// Образец каждой версии формата лежит в репозитории: новые программы
/// обязаны его читать. Образец v1 записан программой с FORMAT_VERSION = 1.
#[test]
fn format_v1_sample_is_readable() {
    let data = include_bytes!("fixtures/scene-v1.glb");
    let read = contract::glb(data).unwrap_or_else(|v| panic!("{v:#?}"));
    let roles: Vec<_> = read.regions.iter().map(|r| r.role).collect();
    assert_eq!(
        roles,
        [Role::After, Role::Added, Role::Removed, Role::Before]
    );
    assert_eq!(read.volume_of(Role::Removed), 512.0);
    let b: Bounds = read.bounds().unwrap();
    assert!(close(b.max, [60.0, 30.0, 20.0]), "{b:?}");
}

/// Записать образец текущей версии: `NONIUS_WRITE_FIXTURE=1 cargo test -p nonius-scene`.
#[test]
fn write_fixture_on_request() {
    if std::env::var_os("NONIUS_WRITE_FIXTURE").is_some() {
        let path = format!(
            "{}/tests/fixtures/scene-v{FORMAT_VERSION}.glb",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::write(path, sample().to_glb()).unwrap();
    }
}
