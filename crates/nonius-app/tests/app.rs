//! Действия `nonius-app` на временном проекте с настоящим git.
//!
//! Модули здесь игрушечные: «STEP» — текстовый файл с объёмом в мм³,
//! отрисовщик пишет `changes.glb` и считает вызовы. Так проверяется сама
//! логика действий и кэша, без OpenCascade.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use nonius_app::{
    App, CompareOptions, Error, GitOptions, ItemState, Mass, MaterialChoice, NoProgress, Progress,
};
use nonius_core::compare::ChangeReport;
use nonius_core::extension::{
    ExtensionPoint, GeometrySource, Module, ModuleManifest, Registry, Renderer, Snapshot,
    API_VERSION,
};
use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::{Project, VersionNumber};

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
        m.source.cad = Some("тест".into());
        Ok(m)
    }
}

/// Отрисовщик: пишет `changes.glb` и считает вызовы.
struct CountingRenderer(ModuleManifest, Arc<AtomicUsize>);

impl Module for CountingRenderer {
    fn manifest(&self) -> &ModuleManifest {
        &self.0
    }
}

impl Renderer for CountingRenderer {
    fn render(
        &self,
        _before: &Snapshot,
        _after: &Snapshot,
        _report: &ChangeReport,
        out_dir: &Path,
    ) -> nonius_core::Result<Vec<PathBuf>> {
        self.1.fetch_add(1, Ordering::SeqCst);
        std::fs::create_dir_all(out_dir).unwrap();
        let p = out_dir.join("changes.glb");
        std::fs::write(&p, b"glTF").unwrap();
        Ok(vec![p])
    }
}

struct Fixture {
    _tmp: tempfile::TempDir,
    app: App,
    renders: Arc<AtomicUsize>,
    root: PathBuf,
}

fn fixture(with_renderer: bool) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("станок");
    let project = Project::init(&root, "Станок").unwrap();
    let renders = Arc::new(AtomicUsize::new(0));
    let mut reg = Registry::with_builtins();
    reg.add_geometry_source(Box::new(VolumeSource(manifest(
        "volume-test",
        ExtensionPoint::GeometrySource,
    ))))
    .unwrap();
    if with_renderer {
        reg.add_renderer(Box::new(CountingRenderer(
            manifest("render-test", ExtensionPoint::Renderer),
            renders.clone(),
        )))
        .unwrap();
    }
    let app = App::with_project(project, reg).with_git_options(GitOptions {
        identity: Some(("Тестировщик".into(), "test@example.com".into())),
    });
    Fixture {
        _tmp: tmp,
        app,
        renders,
        root,
    }
}

/// Импортировать «STEP» с объёмом `volume` в деталь «пластина».
fn import(f: &Fixture, volume: f64) {
    let file = f.root.join("пластина.step");
    std::fs::write(&file, volume.to_string()).unwrap();
    f.app.import(&file, None).unwrap();
    std::fs::remove_file(&file).unwrap();
}

fn v(n: u32) -> VersionNumber {
    VersionNumber::new(n).unwrap()
}

#[test]
fn status_follows_saving() {
    let f = fixture(false);
    assert!(f.app.items().unwrap().is_empty());
    f.app.new_item("рама", ItemKind::Assembly).unwrap();
    import(&f, 1000.0);

    let items = f.app.items().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|s| s.state() == ItemState::New));

    let saved = f.app.save_version("пластина", "Первая версия").unwrap();
    assert_eq!(saved.version, v(1));
    assert_eq!(
        f.app.item_status("пластина").unwrap().state(),
        ItemState::Saved
    );

    import(&f, 1200.0);
    let s = f.app.item_status("пластина").unwrap();
    assert_eq!(s.state(), ItemState::Changed);
    assert!(s.changes.iter().any(|c| c.path == "model.step"), "{s:?}");

    f.app.save_version("пластина", "Толще").unwrap();
    let history = f.app.versions("пластина").unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].summary(), "Толще", "новые первыми");
    assert_eq!(history[0].author, "Тестировщик");

    let back = f.app.restore("пластина", v(1), Some("вернули")).unwrap();
    assert_eq!(back.version, v(3));
    let passport = f.app.passport("пластина", None).unwrap();
    assert_eq!(passport.meta.geometry.volume_mm3, Some(1000.0));

    assert!(matches!(
        f.app.versions("нет-такой"),
        Err(Error::UnknownItem(_))
    ));
}

#[test]
fn mass_only_with_material() {
    let f = fixture(false);
    import(&f, 100_000.0); // 100 × 100 × 10 мм
    let p = f.app.passport("пластина", None).unwrap();
    assert_eq!(p.mass, Mass::NoMaterial, "без материала массы нет");

    let p = f
        .app
        .set_material(
            "пластина",
            Some(MaterialChoice {
                name: "Ст3".into(),
                density_kg_m3: None,
            }),
        )
        .unwrap();
    assert_eq!(
        p.meta.density_kg_m3,
        Some(7850.0),
        "плотность из справочника"
    );
    assert!((p.mass.kg().unwrap() - 0.785).abs() < 1e-9, "{p:?}");
    let saved = std::fs::read_to_string(f.root.join("parts/пластина/meta.yaml")).unwrap();
    assert!(saved.contains("density_kg_m3: 7850"), "{saved}");

    // Новый объём — масса пересчитывается, материал остаётся.
    import(&f, 200_000.0);
    let p = f.app.passport("пластина", None).unwrap();
    assert_eq!(p.meta.material.as_deref(), Some("Ст3"));
    assert!((p.mass.kg().unwrap() - 1.57).abs() < 1e-9, "{p:?}");

    let unknown = MaterialChoice {
        name: "Унобтаниум".into(),
        density_kg_m3: None,
    };
    let p = f.app.set_material("пластина", Some(unknown)).unwrap();
    assert!(matches!(p.mass, Mass::NoDensity { .. }), "{p:?}");
    assert_eq!(p.meta.geometry.mass_kg, None, "старая масса стёрта");

    let own = MaterialChoice {
        name: "Унобтаниум".into(),
        density_kg_m3: Some(1000.0),
    };
    let p = f.app.set_material("пластина", Some(own)).unwrap();
    assert!((p.mass.kg().unwrap() - 0.2).abs() < 1e-9, "{p:?}");

    // Тот же материал без плотности — плотность остаётся прежней.
    let again = MaterialChoice {
        name: "Унобтаниум".into(),
        density_kg_m3: None,
    };
    let p = f.app.set_material("пластина", Some(again)).unwrap();
    assert_eq!(p.meta.density_kg_m3, Some(1000.0), "{p:?}");
    assert!(matches!(p.mass, Mass::Computed { .. }), "{p:?}");

    let bad = MaterialChoice {
        name: "Ст3".into(),
        density_kg_m3: Some(-5.0),
    };
    assert!(matches!(
        f.app.set_material("пластина", Some(bad)),
        Err(Error::BadDensity(_))
    ));

    f.app.save_version("пластина", "свой материал").unwrap();
    let p = f.app.set_material("пластина", None).unwrap();
    assert_eq!(p.mass, Mass::NoMaterial);
    let old = f.app.passport("пластина", Some(v(1))).unwrap();
    assert_eq!(
        old.meta.material.as_deref(),
        Some("Унобтаниум"),
        "версия хранит материал"
    );
}

#[test]
fn compare_uses_cache() {
    let f = fixture(true);
    import(&f, 1000.0);
    assert!(matches!(
        f.app.compare(
            "пластина",
            None,
            None,
            &CompareOptions::default(),
            &NoProgress
        ),
        Err(Error::NoVersions(_))
    ));
    f.app.save_version("пластина", "v1").unwrap();
    import(&f, 1500.0);

    let opts = CompareOptions::default();
    let first = f
        .app
        .compare("пластина", None, None, &opts, &NoProgress)
        .unwrap();
    assert!(!first.from_cache);
    assert_eq!(
        (first.from_label(), first.to_label()),
        ("v1".into(), "текущее".into())
    );
    let text: Vec<String> = first
        .reports
        .iter()
        .flat_map(|r| &r.changes)
        .map(|c| c.to_string())
        .collect();
    assert!(text.iter().any(|t| t.contains("Объём")), "{text:?}");
    assert!(first.file_changes.iter().any(|c| c.path == "model.step"));

    let again = f
        .app
        .compare("пластина", None, None, &opts, &NoProgress)
        .unwrap();
    assert!(again.from_cache);
    assert_eq!(again.reports, first.reports);

    // Картинки: рисуются один раз, дальше из кэша.
    let render = CompareOptions {
        render: true,
        ..CompareOptions::default()
    };
    let r1 = f
        .app
        .compare("пластина", None, None, &render, &NoProgress)
        .unwrap();
    let glb = r1.file("changes.glb").expect("сцена есть");
    assert!(glb.starts_with(f.root.join(".git/nonius/cache")), "{glb:?}");
    let r2 = f
        .app
        .compare("пластина", None, None, &render, &NoProgress)
        .unwrap();
    assert!(r2.from_cache);
    assert_eq!(f.renders.load(Ordering::SeqCst), 1);

    // Деталь поменялась — считаем заново.
    import(&f, 1600.0);
    let r3 = f
        .app
        .compare("пластина", None, None, &render, &NoProgress)
        .unwrap();
    assert!(!r3.from_cache);
    assert_eq!(f.renders.load(Ordering::SeqCst), 2);

    // Сохранённые версии сравниваются так же.
    f.app.save_version("пластина", "v2").unwrap();
    let c = f
        .app
        .compare("пластина", Some(v(1)), Some(v(2)), &opts, &NoProgress)
        .unwrap();
    assert_eq!(c.to_label(), "v2");
    assert!(c.file_changes.is_empty());
    assert!(!c.is_empty());
}

#[test]
fn broken_cache_is_recomputed() {
    let f = fixture(true);
    import(&f, 1000.0);
    f.app.save_version("пластина", "v1").unwrap();
    import(&f, 2000.0);
    let render = CompareOptions {
        render: true,
        ..CompareOptions::default()
    };
    f.app
        .compare("пластина", None, None, &render, &NoProgress)
        .unwrap();

    let cache = f.root.join(".git/nonius/cache");
    for entry in std::fs::read_dir(&cache).unwrap() {
        std::fs::write(entry.unwrap().path().join("cache.yaml"), "schema: [битый").unwrap();
    }
    let c = f
        .app
        .compare("пластина", None, None, &render, &NoProgress)
        .unwrap();
    assert!(!c.from_cache);
    assert!(c.file("changes.glb").unwrap().is_file());
    assert_eq!(f.renders.load(Ordering::SeqCst), 2);
}

/// Отменяет расчёт после первого шага.
struct CancelAfterFirstStep(AtomicBool);

impl Progress for CancelAfterFirstStep {
    fn step(&self, _what: &str) {
        self.0.store(true, Ordering::SeqCst);
    }
    fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[test]
fn cancelled_compare_is_not_cached() {
    let f = fixture(true);
    import(&f, 1000.0);
    f.app.save_version("пластина", "v1").unwrap();
    import(&f, 2000.0);
    let cancel = CancelAfterFirstStep(AtomicBool::new(false));
    let r = f
        .app
        .compare("пластина", None, None, &CompareOptions::default(), &cancel);
    assert!(matches!(r, Err(Error::Cancelled)), "{r:?}");
    let cache = f.root.join(".git/nonius/cache");
    let left = std::fs::read_dir(&cache).map(|d| d.count()).unwrap_or(0);
    assert_eq!(left, 0, "отменённое в кэш не попадает");
}

#[test]
fn render_needs_a_renderer() {
    let f = fixture(false);
    import(&f, 1000.0);
    f.app.save_version("пластина", "v1").unwrap();
    let r = f.app.compare(
        "пластина",
        None,
        None,
        &CompareOptions {
            render: true,
            ..CompareOptions::default()
        },
        &NoProgress,
    );
    assert!(matches!(r, Err(Error::NoRenderer)), "{r:?}");
}

#[test]
fn compare_from_two_threads() {
    let f = fixture(true);
    import(&f, 1000.0);
    f.app.save_version("пластина", "v1").unwrap();
    import(&f, 3000.0);
    let render = CompareOptions {
        render: true,
        ..CompareOptions::default()
    };
    std::thread::scope(|s| {
        let a = s.spawn(|| f.app.compare("пластина", None, None, &render, &NoProgress));
        let b = s.spawn(|| f.app.compare("пластина", None, None, &render, &NoProgress));
        let (a, b) = (a.join().unwrap().unwrap(), b.join().unwrap().unwrap());
        assert_eq!(a.reports, b.reports);
        assert!(a.file("changes.glb").unwrap().is_file());
        assert!(b.file("changes.glb").unwrap().is_file());
    });
}

#[test]
fn import_refuses_unknown_files() {
    let f = fixture(false);
    let txt = f.root.join("заметка.txt");
    std::fs::write(&txt, "x").unwrap();
    assert!(matches!(
        f.app.import(&txt, None),
        Err(Error::UnknownFormat(_))
    ));
    assert!(matches!(
        f.app.import(&f.root.join("нет.step"), None),
        Err(Error::FileNotFound(_))
    ));
    let bad = f.root.join("вал@v2.step");
    std::fs::write(&bad, "1").unwrap();
    let err = f.app.import(&bad, None).unwrap_err();
    assert!(err.to_string().contains("--part"), "{err}");
}
