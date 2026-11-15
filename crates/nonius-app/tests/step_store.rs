//! Склад STEP: один и тот же STEP у модуля перевода второй раз не просят.
//!
//! Модули игрушечные, как в `convert.rs`: «SolidWorks» — текстовый файл с
//! объёмом, перевод считает, сколько раз его позвали.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use nonius_app::{App, GitOptions};
use nonius_core::extension::{
    Converter, ExtensionPoint, GeometrySource, Module, ModuleManifest, Registry, API_VERSION,
};
use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::Project;

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

/// Перевод со счётчиком. «упал» в файле — окончательный отказ, «занят» —
/// «сейчас не вышло».
struct Counting {
    manifest: ModuleManifest,
    calls: Arc<AtomicUsize>,
    /// Условия перевода: `None` — неизвестны.
    stamp: Arc<Mutex<Option<String>>>,
}

impl Module for Counting {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl Converter for Counting {
    fn can_convert(&self, path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "sldprt")
    }
    fn to_step(&self, source: &Path, target: &Path) -> nonius_core::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let text = std::fs::read_to_string(source).unwrap();
        let (module, message) = (self.manifest.id.clone(), "деталь не открылась".to_string());
        if text.contains("упал") {
            return Err(nonius_core::Error::Refused { module, message });
        }
        if text.contains("занят") {
            return Err(nonius_core::Error::Module { module, message });
        }
        std::fs::write(target, text).unwrap();
        Ok(())
    }
    fn stamp(&self, _source: &Path) -> Option<String> {
        self.stamp.lock().unwrap().clone()
    }
}

struct Bench {
    _tmp: tempfile::TempDir,
    dir: PathBuf,
    app: App,
    calls: Arc<AtomicUsize>,
    stamp: Arc<Mutex<Option<String>>>,
}

impl Bench {
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// Файл «SolidWorks» с таким содержимым.
    fn file(&self, name: &str, text: &str) -> PathBuf {
        let file = self.dir.join(name);
        std::fs::write(&file, text).unwrap();
        file
    }
}

fn bench() -> Bench {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("станок");
    let project = Project::init(&root, "Станок").unwrap();
    // Склад живёт в `.git/nonius`: проекту нужна история.
    let git = std::process::Command::new("git")
        .arg("init")
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(git.status.success());
    let calls = Arc::new(AtomicUsize::new(0));
    let stamp = Arc::new(Mutex::new(Some("SolidWorks 2025, STEP AP214".to_string())));
    let mut reg = Registry::with_builtins();
    reg.add_geometry_source(Box::new(VolumeSource(manifest(
        "volume-test",
        ExtensionPoint::GeometrySource,
    ))))
    .unwrap();
    reg.add_converter(Box::new(Counting {
        manifest: manifest("count-test", ExtensionPoint::Converter),
        calls: calls.clone(),
        stamp: stamp.clone(),
    }))
    .unwrap();
    let app = App::with_project(project, reg).with_git_options(GitOptions {
        identity: Some(("Тестировщик".into(), "test@example.com".into())),
        ai: None,
    });
    Bench {
        dir: tmp.path().to_path_buf(),
        _tmp: tmp,
        app,
        calls,
        stamp,
    }
}

#[test]
fn same_content_is_converted_once() {
    let b = bench();
    let file = b.file("вал.sldprt", "1000");
    let done = b.app.import(&file, None).unwrap();
    assert_eq!(done.meta.geometry.volume_mm3, Some(1000.0));
    assert_eq!(b.calls(), 1);

    // Тот же файл ещё раз и его копия под другим именем в другой папке с
    // кириллицей и пробелом — STEP со склада.
    b.app.import(&file, None).unwrap();
    std::fs::create_dir(b.dir.join("КТМ 640")).unwrap();
    let copy = b.file("КТМ 640/вал копия.sldprt", "1000");
    let done = b.app.import(&copy, Some("вал-2")).unwrap();
    assert_eq!(done.warnings, [] as [&str; 0]);
    assert_eq!(done.meta.geometry.volume_mm3, Some(1000.0));
    assert!(done.item.dir.join("model.step").is_file());
    assert_eq!(b.calls(), 1, "повторный запрос модуль не зовёт");
    assert!(b.app.steps_size() > 0);

    // Содержимое изменилось — STEP новый.
    std::fs::write(&file, "1200").unwrap();
    let done = b.app.import(&file, None).unwrap();
    assert_eq!(done.meta.geometry.volume_mm3, Some(1200.0));
    assert_eq!(b.calls(), 2);

    // Склад — кэш: очистили — всё получится заново.
    assert!(b.app.forget_steps() > 0);
    assert_eq!(b.app.steps_size(), 0);
    b.app.import(&file, None).unwrap();
    assert_eq!(b.calls(), 3);
}

#[test]
fn final_refusal_is_remembered_and_a_passing_one_is_not() {
    let b = bench();
    let broken = b.file("кривая.sldprt", "упал");
    let done = b.app.import(&broken, None).unwrap();
    assert!(
        done.warnings[0].contains("не открылась"),
        "{:?}",
        done.warnings
    );
    assert_eq!(b.calls(), 1);
    // Второй раз модуль не будят, а человеку сказано, что это память.
    let done = b.app.import(&broken, None).unwrap();
    assert_eq!(b.calls(), 1);
    assert!(
        done.warnings[0].contains("не открылась") && done.warnings[0].contains("прошлый раз"),
        "{:?}",
        done.warnings
    );
    // Файл поправили — пробуем снова.
    std::fs::write(&broken, "700").unwrap();
    let done = b.app.import(&broken, None).unwrap();
    assert_eq!(done.meta.geometry.volume_mm3, Some(700.0));
    assert_eq!(b.calls(), 2);

    // «Сейчас не вышло» не запоминается.
    let busy = b.file("вал.sldprt", "занят");
    b.app.import(&busy, None).unwrap();
    b.app.import(&busy, None).unwrap();
    assert_eq!(b.calls(), 4);
}

#[test]
fn conditions_of_conversion_are_part_of_the_key() {
    let b = bench();
    let file = b.file("вал.sldprt", "1000");
    b.app.import(&file, None).unwrap();
    // Другая версия SolidWorks или другие настройки STEP — другой STEP.
    *b.stamp.lock().unwrap() = Some("SolidWorks 2026, STEP AP203".into());
    b.app.import(&file, None).unwrap();
    assert_eq!(b.calls(), 2);
    b.app.import(&file, None).unwrap();
    assert_eq!(b.calls(), 2);
    // Условия неизвестны (сборка, чьи детали не нашлись) — склад не при чём.
    *b.stamp.lock().unwrap() = None;
    b.app.import(&file, None).unwrap();
    b.app.import(&file, None).unwrap();
    assert_eq!(b.calls(), 4);
}
