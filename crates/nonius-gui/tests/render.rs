//! Отрисовка на видеокарте без окна. Где видеокарты нет совсем (даже
//! программной), тест пропускается с сообщением.

use nonius_gui::view::gpu::Gpu;
use nonius_gui::view::{Camera, Look, Mode, Model, StdView};
use nonius_scene::{Mesh, Region, Role, Scene};

/// Коробка, как её режет OpenCascade: у каждой грани свои вершины.
fn block(lo: [f32; 3], hi: [f32; 3]) -> Mesh {
    let mut m = Mesh::default();
    let c =
        |x: usize, y: usize, z: usize| [[lo[0], hi[0]][x], [lo[1], hi[1]][y], [lo[2], hi[2]][z]];
    let faces = [
        [c(0, 0, 0), c(0, 1, 0), c(1, 1, 0), c(1, 0, 0)],
        [c(0, 0, 1), c(1, 0, 1), c(1, 1, 1), c(0, 1, 1)],
        [c(0, 0, 0), c(1, 0, 0), c(1, 0, 1), c(0, 0, 1)],
        [c(0, 1, 0), c(0, 1, 1), c(1, 1, 1), c(1, 1, 0)],
        [c(0, 0, 0), c(0, 0, 1), c(0, 1, 1), c(0, 1, 0)],
        [c(1, 0, 0), c(1, 1, 0), c(1, 1, 1), c(1, 0, 1)],
    ];
    for f in faces {
        let base = m.positions.len() as u32;
        m.positions.extend(f);
        m.triangles.push([base, base + 1, base + 2]);
        m.triangles.push([base, base + 2, base + 3]);
    }
    m
}

/// Пластина 60×8×30 (Y вверх), сверху — бобышка, сбоку убран кусок.
fn scene() -> Scene {
    let region = |role, lo, hi| Region {
        role,
        volume_mm3: 1.0,
        mesh: block(lo, hi),
    };
    Scene {
        regions: vec![
            region(Role::After, [0.0, 0.0, 0.0], [60.0, 8.0, 30.0]),
            region(Role::Before, [0.0, 0.0, 0.0], [60.0, 8.0, 30.0]),
            region(Role::Added, [40.0, 8.0, 8.0], [52.0, 20.0, 22.0]),
            region(Role::Removed, [5.0, 0.0, 10.0], [15.0, 8.0, 20.0]),
        ],
    }
}

fn count(img: &nonius_gui::view::gpu::Image, test: impl Fn([u8; 4]) -> bool) -> usize {
    img.rgba
        .chunks_exact(4)
        .filter(|p| test([p[0], p[1], p[2], p[3]]))
        .count()
}

#[test]
fn renders_changes_in_color() {
    let Some(mut gpu) = Gpu::headless() else {
        eprintln!("видеокарты нет — тест отрисовки пропущен");
        return;
    };
    let model = Model::new(&scene()).unwrap();
    gpu.upload(&model.parts);
    let (w, h) = (320u32, 200u32);
    let aspect = w as f32 / h as f32;
    let mut cam = Camera {
        orientation: StdView::Iso.orientation(),
        ..Camera::default()
    };
    cam.fit(&model.bounds, aspect, 0.0);

    let look = Look::default();
    let shot = |gpu: &mut Gpu, mode| {
        let frame = nonius_gui::view::gpu::Frame {
            view_proj: cam.view_proj(aspect, &model.bounds),
            light: cam.orientation * glam::Vec3::new(-0.35, 0.45, 1.0).normalize(),
            draws: model.draws(mode, &look, None),
            edges: true,
        };
        gpu.render(&frame, [w, h]);
        gpu.read_image().unwrap().unwrap()
    };
    let green = |p: [u8; 4]| p[1] as i32 - p[0] as i32 > 60 && p[1] as i32 - p[2] as i32 > 30;
    let red = |p: [u8; 4]| p[0] as i32 - p[1] as i32 > 80;

    let changes = shot(&mut gpu, Mode::Changes);
    if let Some(dir) = std::env::var_os("NONIUS_RENDER_DIR") {
        changes
            .save_png(&std::path::Path::new(&dir).join("changes.png"))
            .unwrap();
    }
    assert_eq!((changes.width, changes.height), (w, h));
    assert_eq!(
        changes.pixel(0, 0)[..3],
        nonius_gui::view::gpu::BACKGROUND,
        "фон в углу"
    );
    assert!(count(&changes, green) > 300, "зелёного мало");
    assert!(count(&changes, red) > 100, "красного мало");

    let after = shot(&mut gpu, Mode::After);
    assert!(count(&after, green) > 300);
    assert_eq!(count(&after, red), 0, "в «Стало» убранного нет");
    let before = shot(&mut gpu, Mode::Before);
    assert_eq!(count(&before, green), 0, "в «Было» добавленного нет");
    assert!(count(&before, red) > 100);
}
