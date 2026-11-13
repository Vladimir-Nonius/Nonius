//! Два правила сравнения:
//! - допуск проекта доходит до отрисовщиков (картинка и сцена считаются с
//!   тем же допуском, что и отчёт);
//! - расчёт, который успел закончиться, остаётся в кэше, даже если человек
//!   нажал «Остановить».
//!
//! Модули игрушечные, как в `app.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nonius_app::{App, CompareOptions, Error, GitOptions, NoProgress, Progress, ProjectSettings};
use nonius_core::compare::{ChangeReport, CompareContext};
use nonius_core::extension::{
    ExtensionPoint, GeometrySource, Module, ModuleManifest, Registry, Renderer, Snapshot,
    API_VERSION,
};
use nonius_core::meta::{ItemKind, PartMeta};

fn manifest(id: &str, point: ExtensionPoint) -> ModuleManifest {
    ModuleManifest {
        id: id.into(),
        name: id.into(),
        version: "1.0.0".into(),
        api: API_VERSION,
        points: vec![point],
        permissions: vec![],
    }
}

/// «STEP», в котором записан объём.
struct VolumeSource(ModuleManifest);

impl Module for VolumeSource {
    fn manifest(&self) -> &ModuleManifest {
        &self.0
    }
}

impl GeometrySource for VolumeSource {
    fn can_read(&self, path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "step")
    }
    fn read(&self, path: &Path) -> nonius_core::Result<PartMeta> {
        let text = std::fs::read_to_string(path).unwrap();
        let mut m = PartMeta::new(ItemKind::Part);
        m.geometry.volume_mm3 = text.trim().parse().ok();
        Ok(m)
    }
}

/// Отрисовщик, который записывает, с каким допуском его позвали.
struct ToleranceRecorder(ModuleManifest, Arc<Mutex<Vec<f64>>>);

impl Module for ToleranceRecorder {
    fn manifest(&self) -> &ModuleManifest {
        &self.0
    }
}

impl Renderer for ToleranceRecorder {
    fn render(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        report: &ChangeReport,
        out_dir: &Path,
    ) -> nonius_core::Result<Vec<PathBuf>> {
        self.render_with(before, after, report, &CompareContext::default(), out_dir)
    }

    fn render_with(
        &self,
        _before: &Snapshot,
        _after: &Snapshot,
        _report: &ChangeReport,
        ctx: &CompareContext,
        out_dir: &Path,
    ) -> nonius_core::Result<Vec<PathBuf>> {
        self.1.lock().unwrap().push(ctx.tolerance_mm);
        std::fs::create_dir_all(out_dir).unwrap();
        let p = out_dir.join("changes.glb");
        std::fs::write(&p, b"glTF").unwrap();
        Ok(vec![p])
    }
}

fn open(root: &Path, seen: &Arc<Mutex<Vec<f64>>>) -> App {
    let mut reg = Registry::with_builtins();
    reg.add_geometry_source(Box::new(VolumeSource(manifest(
        "volume-test",
        ExtensionPoint::GeometrySource,
    ))))
    .unwrap();
    reg.add_renderer(Box::new(ToleranceRecorder(
        manifest("tolerance-test", ExtensionPoint::Renderer),
        seen.clone(),
    )))
    .unwrap();
    App::open(root, reg).unwrap().with_git_options(GitOptions {
        identity: Some(("Тестировщик".into(), "test@example.com".into())),
        ai: None,
    })
}

/// Проект с деталью «пластина»: версия v1 и несохранённое изменение.
fn project(root: &Path, seen: &Arc<Mutex<Vec<f64>>>) -> App {
    nonius_app::create_project(root, Some("Станок")).unwrap();
    let app = open(root, seen);
    let file = root.join("пластина.step");
    std::fs::write(&file, "1000").unwrap();
    app.import(&file, None).unwrap();
    app.save_version("пластина", "v1").unwrap();
    std::fs::write(&file, "2000").unwrap();
    app.import(&file, None).unwrap();
    std::fs::remove_file(&file).unwrap();
    app
}

#[test]
fn project_tolerance_reaches_renderers() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("станок");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let app = project(&root, &seen);
    let changed = app
        .set_project_settings(&ProjectSettings {
            name: "Станок".into(),
            tolerance_mm: 0.5,
        })
        .unwrap();
    assert!(changed);

    // Настройки видит заново открытый проект.
    let app = open(&root, &seen);
    let render = CompareOptions {
        render: true,
        ..CompareOptions::default()
    };
    app.compare("пластина", None, None, &render, &NoProgress)
        .unwrap();
    assert_eq!(*seen.lock().unwrap(), [0.5]);
}

/// Нажимает «Остановить», как только начался сам расчёт.
struct CancelWhileComparing(AtomicBool);

impl Progress for CancelWhileComparing {
    fn step(&self, what: &str) {
        if what == "Сравниваю" {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[test]
fn finished_work_survives_cancel() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("станок");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let app = project(&root, &seen);

    let opts = CompareOptions::default();
    let cancel = CancelWhileComparing(AtomicBool::new(false));
    let stopped = app.compare("пластина", None, None, &opts, &cancel);
    assert!(matches!(stopped, Err(Error::Cancelled)), "{stopped:?}");

    // Человек остановил показ, но расчёт успел закончиться: во второй раз
    // он берётся готовым.
    let again = app
        .compare("пластина", None, None, &opts, &NoProgress)
        .unwrap();
    assert!(again.from_cache);
    assert!(!again.reports.is_empty());
}
