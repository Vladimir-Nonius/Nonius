//! Разбор папки от начала до конца: настоящие файлы на диске, настоящий git,
//! игрушечные модули. «STEP» — текст с объёмом; связи файлов выдаёт
//! поддельный модуль (как их выдавал бы SolidWorks).

use std::path::{Path, PathBuf};

use nonius_app::{App, Error, GitOptions, NoProgress};
use nonius_core::contract;
use nonius_core::extension::{
    ExtensionPoint, GeometrySource, LinkSource, Module, ModuleManifest, Registry, API_VERSION,
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

/// Связи: в файле сборки или чертежа перечислены имена файлов, по строке.
struct ListedLinks(ModuleManifest);

impl Module for ListedLinks {
    fn manifest(&self) -> &ModuleManifest {
        &self.0
    }
}

impl LinkSource for ListedLinks {
    fn knows(&self, path: &Path) -> bool {
        path.extension()
            .is_some_and(|e| e == "sldasm" || e == "slddrw")
    }
    fn links(&self, files: &[PathBuf]) -> nonius_core::Result<Vec<Vec<PathBuf>>> {
        Ok(files
            .iter()
            .map(|f| {
                let dir = f.parent().unwrap();
                std::fs::read_to_string(f)
                    .unwrap_or_default()
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| dir.join(l.trim()))
                    .collect()
            })
            .collect())
    }
}

fn links() -> ListedLinks {
    ListedLinks(manifest("links-test", ExtensionPoint::Links))
}

fn app(root: &Path) -> App {
    let project = Project::init(root, "Станок").unwrap();
    let mut reg = Registry::with_builtins();
    reg.add_geometry_source(Box::new(VolumeSource(manifest(
        "volume-test",
        ExtensionPoint::GeometrySource,
    ))))
    .unwrap();
    reg.add_link_source(Box::new(links())).unwrap();
    App::with_project(project, reg).with_git_options(GitOptions {
        identity: Some(("Тестировщик".into(), "test@example.com".into())),
        ai: None,
    })
}

/// Папка, как у людей: всё вперемешку.
fn messy(dir: &Path) {
    let put = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
    std::fs::create_dir_all(dir).unwrap();
    put("вал.step", "1000");
    put("вал_v2.step", "1200");
    put("втулка.step", "300");
    put("чертеж вала.slddrw", "вал_v2.step\n");
    put("втулка.pdf", "бумага");
    put("насос.sldasm", "вал_v2.step\nвтулка.step\n");
    put("расчёт.log", "не наше");
    put("~$вал.step", "временный файл SolidWorks");
}

#[test]
fn fake_links_pass_contract() {
    let tmp = tempfile::tempdir().unwrap();
    messy(tmp.path());
    if let Err(v) = contract::link_source(&links(), &tmp.path().join("насос.sldasm")) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
}

#[test]
fn folder_becomes_a_project() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("свалка");
    messy(&source);
    let app = app(&tmp.path().join("станок"));

    // План: ничего не изменено, всё объяснено.
    let plan = app.intake_plan(&source).unwrap();
    let names: Vec<&str> = plan.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["вал", "втулка", "насос"]);
    let shaft = &plan.items[0];
    assert_eq!(shaft.versions.len(), 2);
    assert!(shaft.guessed);
    assert_eq!(shaft.docs.len(), 1, "чертёж пришёл по ссылке: {shaft:?}");
    assert_eq!(plan.items[2].kind, ItemKind::Assembly);
    assert_eq!(plan.left.len(), 1, "{:?}", plan.left);
    assert!(app.items().unwrap().is_empty(), "план ничего не создаёт");

    // Выполнение: детали, версии, бумаги.
    let done = app.intake_apply(&plan, &NoProgress).unwrap();
    assert_eq!(done.failed, []);
    assert_eq!(
        done.saved,
        [("вал".into(), 2), ("втулка".into(), 1), ("насос".into(), 1)]
    );
    let versions = app.versions("вал").unwrap();
    assert_eq!(versions.len(), 2);
    let shaft = app.item("вал").unwrap();
    let meta = app.project().load_meta(&shaft).unwrap();
    assert_eq!(meta.geometry.volume_mm3, Some(1200.0), "последняя версия");
    assert!(shaft.dir.join("docs/чертеж вала.slddrw").is_file());
    assert!(app
        .item("втулка")
        .unwrap()
        .dir
        .join("docs/втулка.pdf")
        .is_file());
    assert!(app.items().unwrap().iter().all(|i| i.changes.is_empty()));

    // Исходная папка цела.
    assert_eq!(std::fs::read_dir(&source).unwrap().count(), 8);

    // Второй разбор той же папки ничего не дублирует.
    let again = app.intake_plan(&source).unwrap();
    assert!(again.items.is_empty(), "{:?}", again.items);
    assert!(again.left.iter().any(|l| l.why.contains("уже есть")));
}

#[test]
fn project_folder_itself_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("станок");
    let app = app(&root);
    assert!(matches!(
        app.intake_plan(&root),
        Err(Error::IntakeInsideProject(_))
    ));
    assert!(matches!(
        app.intake_plan(tmp.path()),
        Err(Error::IntakeInsideProject(_))
    ));
    assert!(matches!(
        app.intake_plan(&tmp.path().join("нет такой")),
        Err(Error::FileNotFound(_))
    ));
}
