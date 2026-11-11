//! Разбор папки от начала до конца: настоящие файлы на диске, настоящий git,
//! игрушечные модули. «STEP» — текст с объёмом; связи файлов выдаёт
//! поддельный модуль (как их выдавал бы SolidWorks).

use std::path::{Path, PathBuf};

use nonius_app::{App, Error, GitOptions, NoProgress, ShapeCheck, ShapeVerdict};
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

/// «STEP», в котором записан объём, а за ним, если есть, три габарита.
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
        let numbers: Vec<f64> = text
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        m.geometry.volume_mm3 = numbers.first().copied();
        if let [_, x, y, z] = numbers[..] {
            m.geometry.bbox = Some(nonius_core::meta::BoundingBox {
                min: [0.0; 3],
                max: [x, y, z],
            });
        }
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
fn shape_of_the_parts_checks_the_names() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("свалка");
    std::fs::create_dir_all(&source).unwrap();
    let put = |name: &str, text: &str| std::fs::write(source.join(name), text).unwrap();
    put("вал.step", "10000 20 20 100");
    put("вал_v2.step", "10400 20 20 100");
    put("кожух.step", "5000 2 80 120");
    put("кожух_v2.step", "900 10 10 30");
    put("люк.step", "5100 2 80 121");
    let app = app(&tmp.path().join("станок"));
    let names = |plan: &nonius_app::IntakePlan| -> Vec<String> {
        plan.items.iter().map(|i| i.name.clone()).collect()
    };

    // Без проверки формой — как раньше, по одним именам.
    let quick = app
        .intake_plan_with(&source, ShapeCheck::Off, &NoProgress)
        .unwrap();
    assert_eq!(names(&quick), ["вал", "кожух", "люк"]);
    assert!(quick.items.iter().all(|i| i.shape.is_none()));

    // Обычный разбор: вал подтверждён, кожух — две разные детали.
    let plan = app.intake_plan(&source).unwrap();
    assert_eq!(names(&plan), ["вал", "кожух", "кожух_v2", "люк"]);
    assert_eq!(plan.items[0].shape, Some(ShapeVerdict::Confirmed));
    assert_eq!(plan.items[2].shape, Some(ShapeVerdict::Different));
    assert!(plan.items[3].why.is_empty(), "люк не смотрели");

    // Глубокий: смотрит все детали и замечает, что люк похож на кожух.
    let deep = app
        .intake_plan_with(&source, ShapeCheck::All, &NoProgress)
        .unwrap();
    assert!(deep.items[3].why[0].contains("похожа на «кожух»"));

    let done = app.intake_apply(&plan, &NoProgress).unwrap();
    assert_eq!(done.failed, []);
    assert_eq!(app.versions("вал").unwrap().len(), 2);
    assert_eq!(app.versions("кожух_v2").unwrap().len(), 1);
}

#[test]
fn answers_reshape_the_plan() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("свалка");
    std::fs::create_dir_all(&source).unwrap();
    let put = |name: &str, text: &str| std::fs::write(source.join(name), text).unwrap();
    put("кожух.step", "5000 2 80 120");
    put("кожух_v2.step", "900 10 10 30");
    put("люк.step", "5100 2 80 121");
    let app = app(&tmp.path().join("станок"));

    // Папка изучается один раз; план по ответам пересчитывается мгновенно.
    let study = app
        .intake_study(&source, ShapeCheck::All, &NoProgress)
        .unwrap();
    let plan = study.plan(&[]);
    let ids: Vec<&str> = plan.questions.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, ["кожух|кожух_v2", "кожух|люк"]);
    assert_eq!(plan.items.len(), 3);

    let plan = study.plan(&[("кожух|люк".into(), true), ("кожух|кожух_v2".into(), false)]);
    assert!(plan.questions.is_empty(), "{:?}", plan.questions);
    // Кожух и люк стали одной деталью (имя — от более нового файла).
    assert_eq!(plan.items.len(), 2);
    let merged = plan.items.iter().find(|i| i.versions.len() == 2).unwrap();
    assert!(merged.why.iter().any(|w| w.contains("вы ответили")));
    assert!(!merged.guessed);

    let done = app.intake_apply(&plan, &NoProgress).unwrap();
    assert_eq!(done.failed, []);
    assert_eq!(app.versions(&merged.name).unwrap().len(), 2);
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
