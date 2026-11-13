//! Импорт файла SolidWorks с модулем перевода в STEP (точка `Converter`).
//!
//! Модули игрушечные: «SolidWorks» — текстовый файл с объёмом в мм³,
//! перевод переписывает его в «STEP», источник геометрии читает объём.
//! Настоящий SolidWorks здесь не нужен.

use std::path::Path;

use nonius_app::{App, GitOptions};
use nonius_core::extension::{
    Converter, ExtensionPoint, GeometrySource, Module, ModuleManifest, Registry, API_VERSION,
};
use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::{contract, Project};

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

/// «STEP», в котором после заголовка записан объём.
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
        m.geometry.volume_mm3 = text.trim_start_matches(HEADER).trim().parse().ok();
        m.source.cad = Some("тест".into());
        Ok(m)
    }
}

const HEADER: &str = "ISO-10303-21;";

/// Перевод: переписывает число из «детали SolidWorks» в «STEP».
/// Файл со словом «сломан» перевести не может.
struct FakeConverter(ModuleManifest);

impl Module for FakeConverter {
    fn manifest(&self) -> &ModuleManifest {
        &self.0
    }
}

impl Converter for FakeConverter {
    fn can_convert(&self, path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "sldprt")
    }
    fn to_step(&self, source: &Path, target: &Path) -> nonius_core::Result<()> {
        let text = std::fs::read_to_string(source).unwrap();
        if text.contains("сломан") {
            return Err(nonius_core::Error::Module {
                module: self.0.id.clone(),
                message: "деталь не открылась".into(),
            });
        }
        std::fs::write(target, format!("{HEADER}{text}")).unwrap();
        Ok(())
    }
}

fn converter() -> FakeConverter {
    FakeConverter(manifest("convert-test", ExtensionPoint::Converter))
}

fn app(root: &Path, with_converter: bool) -> App {
    let project = Project::init(root, "Станок").unwrap();
    let mut reg = Registry::with_builtins();
    reg.add_geometry_source(Box::new(VolumeSource(manifest(
        "volume-test",
        ExtensionPoint::GeometrySource,
    ))))
    .unwrap();
    if with_converter {
        reg.add_converter(Box::new(converter())).unwrap();
    }
    App::with_project(project, reg).with_git_options(GitOptions {
        identity: Some(("Тестировщик".into(), "test@example.com".into())),
        ai: None,
    })
}

fn solidworks_file(dir: &Path, text: &str) -> std::path::PathBuf {
    let file = dir.join("вал.sldprt");
    std::fs::write(&file, text).unwrap();
    file
}

#[test]
fn fake_converter_passes_contract() {
    let tmp = tempfile::tempdir().unwrap();
    let sample = solidworks_file(tmp.path(), "1000");
    if let Err(v) = contract::converter(&converter(), &sample, tmp.path()) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
}

#[test]
fn solidworks_file_brings_its_step() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(&tmp.path().join("станок"), true);
    let file = solidworks_file(tmp.path(), "1000");

    let done = app.import(&file, None).unwrap();
    assert_eq!(done.target, "model.sldprt");
    assert_eq!(done.warnings, [] as [&str; 0]);
    assert_eq!(done.meta.geometry.volume_mm3, Some(1000.0));
    assert!(done.item.dir.join("model.sldprt").is_file());
    assert!(done.item.dir.join("model.step").is_file());

    // Новая версия детали — новый STEP и новый объём, без предупреждений.
    std::fs::write(&file, "1200").unwrap();
    let done = app.import(&file, None).unwrap();
    assert_eq!(done.warnings, [] as [&str; 0]);
    assert_eq!(done.meta.geometry.volume_mm3, Some(1200.0));
    let step = std::fs::read_to_string(done.item.dir.join("model.step")).unwrap();
    assert!(step.ends_with("1200"), "{step}");
}

#[test]
fn failed_conversion_keeps_the_old_way() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(&tmp.path().join("станок"), true);
    let file = solidworks_file(tmp.path(), "1000");
    app.import(&file, None).unwrap();

    // Перевод не удался: файл всё равно принят, STEP и объём — прежние,
    // и об этом сказано.
    std::fs::write(&file, "сломан").unwrap();
    let done = app.import(&file, None).unwrap();
    assert!(
        done.warnings[0].contains("STEP не получен") && done.warnings[0].contains("не открылась"),
        "{:?}",
        done.warnings
    );
    assert!(
        done.warnings.iter().any(|w| w.contains("по прежнему STEP")),
        "{:?}",
        done.warnings
    );
    assert_eq!(done.meta.geometry.volume_mm3, Some(1000.0));
    let kept = std::fs::read_to_string(done.item.dir.join("model.sldprt")).unwrap();
    assert_eq!(kept, "сломан");
}

#[test]
fn without_converter_nothing_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(&tmp.path().join("станок"), false);
    let file = solidworks_file(tmp.path(), "1000");
    let done = app.import(&file, None).unwrap();
    assert_eq!(done.warnings, [] as [&str; 0]);
    assert!(!done.item.dir.join("model.step").exists());
    assert_eq!(done.meta.geometry.volume_mm3, None);
}
