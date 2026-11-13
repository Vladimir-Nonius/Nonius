//! Разбор папки от начала до конца: настоящие файлы на диске, настоящий git,
//! игрушечные модули. «STEP» — текст с объёмом; связи файлов выдаёт
//! поддельный модуль (как их выдавал бы SolidWorks); беглый взгляд на форму
//! читает те же числа из «файла SolidWorks».

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nonius_app::{App, Error, GitOptions, NoProgress, ShapeCheck, ShapeVerdict};
use nonius_core::compare::{Change, ChangeKind, ChangeReport, CompareContext};
use nonius_core::contract;
use nonius_core::extension::{
    Comparator, Converter, ExtensionPoint, GeometrySource, LinkSource, Module, ModuleManifest,
    Registry, ShapeGlance, Snapshot, API_VERSION,
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

/// Беглый взгляд: «файл SolidWorks», в котором записаны объём и габариты.
/// Файл без чисел — как настоящий без сетки: отказ.
struct TextGlance(ModuleManifest);

impl Module for TextGlance {
    fn manifest(&self) -> &ModuleManifest {
        &self.0
    }
}

impl ShapeGlance for TextGlance {
    fn can_glance(&self, path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "sldprt")
    }
    fn glance(&self, path: &Path) -> nonius_core::Result<nonius_core::meta::GeometrySummary> {
        // Вторая строка — то, что сказал бы о файле SolidWorks (см. Exporter).
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let numbers: Vec<f64> = text
            .lines()
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        let [volume, x, y, z] = numbers[..] else {
            return Err(nonius_core::Error::Module {
                module: "glance-test".into(),
                message: "в файле нет сетки".into(),
            });
        };
        Ok(nonius_core::meta::GeometrySummary {
            volume_mm3: Some(volume),
            bbox: Some(nonius_core::meta::BoundingBox {
                min: [0.0; 3],
                max: [x, y, z],
            }),
            ..Default::default()
        })
    }
}

/// Быстрое сравнение: говорит, как изменился текст «STEP».
struct TextComparator(ModuleManifest);

impl Module for TextComparator {
    fn manifest(&self) -> &ModuleManifest {
        &self.0
    }
}

impl Comparator for TextComparator {
    fn compare(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        _: &CompareContext,
    ) -> nonius_core::Result<ChangeReport> {
        let text = |s: &Snapshot| std::fs::read_to_string(s.dir.join("model.step")).ok();
        let mut report = ChangeReport::empty("text-test");
        if let (Some(was), Some(now)) = (text(before), text(after)) {
            if was != now {
                report.changes.push(Change {
                    kind: ChangeKind::Modified,
                    subject: "запись".into(),
                    before: Some(was),
                    after: Some(now),
                });
            }
        }
        Ok(report)
    }
    fn quick(&self) -> bool {
        true
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
    app_with(root, |_| {})
}

/// То же, а модули сверх обычных добавит `more`.
fn app_with(root: &Path, more: impl FnOnce(&mut Registry)) -> App {
    let project = Project::init(root, "Станок").unwrap();
    let mut reg = Registry::with_builtins();
    more(&mut reg);
    reg.add_geometry_source(Box::new(VolumeSource(manifest(
        "volume-test",
        ExtensionPoint::GeometrySource,
    ))))
    .unwrap();
    reg.add_link_source(Box::new(links())).unwrap();
    reg.add_glance(Box::new(TextGlance(manifest(
        "glance-test",
        ExtensionPoint::Glance,
    ))))
    .unwrap();
    reg.add_comparator(Box::new(TextComparator(manifest(
        "text-test",
        ExtensionPoint::Comparator,
    ))))
    .unwrap();
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
    // Главная улика — что изменилось, словами: и в плане, и в вопросе.
    let said = "что изменилось, «вал» → «вал_v2»: запись: 10000 20 20 100 → 10400 20 20 100";
    assert!(
        plan.items[0].why.iter().any(|w| w == said),
        "{:?}",
        plan.items[0].why
    );
    let facts = &plan.questions[0].facts;
    assert!(
        facts
            .iter()
            .any(|f| f.starts_with("что изменилось: запись")),
        "{facts:?}"
    );
    assert!(quick.items[0]
        .why
        .iter()
        .all(|w| !w.contains("что изменилось")));
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
fn shape_is_read_from_the_cad_file_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("свалка");
    std::fs::create_dir_all(&source).unwrap();
    let put = |name: &str, text: &str| std::fs::write(source.join(name), text).unwrap();
    // Модуля перевода в STEP нет вовсе: SolidWorks не спрашивают.
    put("вал.sldprt", "10000 20 20 100");
    put("вал_v2.sldprt", "10400 20 20 100");
    // В файле нет сетки — форму берут из STEP рядом, с запасом на способ.
    put("ось.sldprt", "файл без сетки");
    put("ось.step", "700 5 5 40");
    put("ось_v2.sldprt", "704 5 5 40");
    // Форму взять неоткуда — догадка остаётся догадкой.
    put("штифт.sldprt", "файл без сетки");
    put("штифт_v2.sldprt", "файл без сетки");
    let app = app(&tmp.path().join("станок"));
    assert_eq!(
        contract::glance(
            &TextGlance(manifest("glance-test", ExtensionPoint::Glance)),
            &source.join("вал.sldprt")
        ),
        Ok(())
    );

    let plan = app.intake_plan(&source).unwrap();
    let shape = |name: &str| plan.items.iter().find(|i| i.name == name).unwrap().shape;
    assert_eq!(shape("вал"), Some(ShapeVerdict::Confirmed));
    assert_eq!(shape("ось"), Some(ShapeVerdict::Copy), "точная и беглая");
    assert_eq!(shape("штифт"), None);
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
fn copies_of_the_project_fold_into_one() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("станок");
    let put = |dir: &str, name: &str, text: &str| {
        std::fs::create_dir_all(source.join(dir)).unwrap();
        std::fs::write(source.join(dir).join(name), text).unwrap();
    };
    // Вал в копии не трогали; втулку доработали.
    put("рабочая версия", "вал.step", "10000 20 20 100");
    put("готовая версия", "вал.step", "10000 20 20 100");
    put("рабочая версия", "втулка.step", "300 5 10 10");
    put("готовая версия", "втулка.step", "310 5 10 10");
    let app = app(&tmp.path().join("проект"));

    let plan = app.intake_plan(&source).unwrap();
    let names: Vec<&str> = plan.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["вал", "втулка"], "{:?}", plan.items);
    let (shaft, bush) = (&plan.items[0], &plan.items[1]);
    assert_eq!(shaft.versions.len(), 1);
    assert!(shaft.why[0].contains("та же деталь лежит ещё тут"));
    assert_eq!(bush.versions.len(), 2);
    assert_eq!(bush.shape, Some(ShapeVerdict::Confirmed));

    // Без проверки формой одинаковые файлы всё равно склеиваются.
    let quick = app
        .intake_plan_with(&source, ShapeCheck::Off, &NoProgress)
        .unwrap();
    assert_eq!(quick.items.len(), 3, "вал один, втулок две");

    let done = app.intake_apply(&plan, &NoProgress).unwrap();
    assert_eq!(done.failed, []);
    assert_eq!(app.versions("вал").unwrap().len(), 1);
    assert_eq!(app.versions("втулка").unwrap().len(), 2);
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

/// «SolidWorks»: STEP из «файла SolidWorks» — его последняя строка (там,
/// где беглый взгляд и SolidWorks расходятся, вторая строка — точная).
/// Помнит, какие файлы переводил.
struct Exporter(ModuleManifest, Arc<Mutex<Vec<String>>>);

impl Module for Exporter {
    fn manifest(&self) -> &ModuleManifest {
        &self.0
    }
}

impl Converter for Exporter {
    fn can_convert(&self, path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "sldprt")
    }
    fn to_step(&self, source: &Path, target: &Path) -> nonius_core::Result<()> {
        let name = source.file_name().unwrap().to_string_lossy().into_owned();
        self.1.lock().unwrap().push(name);
        let text = std::fs::read_to_string(source).unwrap();
        std::fs::write(target, text.lines().last().unwrap()).unwrap();
        Ok(())
    }
}

#[test]
fn quick_plan_first_then_disputed_parts_are_refined() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("свалка");
    std::fs::create_dir_all(&source).unwrap();
    let put = |name: &str, text: &str| std::fs::write(source.join(name), text).unwrap();
    // По сетке объём вала ушёл на 25,4 % — чуть за порогом «похожи»;
    // SolidWorks знает точно: на 16,7 %.
    put("вал.sldprt", "10000 20 20 100");
    put("вал_v2.sldprt", "13400 20 20 100\n12000 20 20 100");
    // Ось далеко от порогов: беглой формы достаточно.
    put("ось.sldprt", "1000 5 5 40");
    put("ось_v2.sldprt", "1050 5 5 40");
    let converted = Arc::new(Mutex::new(Vec::new()));
    let exporter = Exporter(
        manifest("sw-test", ExtensionPoint::Converter),
        converted.clone(),
    );
    let app = app_with(&tmp.path().join("станок"), |reg| {
        reg.add_converter(Box::new(exporter)).unwrap();
    });
    // Память о форме живёт в `.git/nonius`: у проекта должна быть история.
    let git = std::process::Command::new("git")
        .arg("init")
        .current_dir(tmp.path().join("станок"))
        .output()
        .unwrap();
    assert!(git.status.success());

    // Первый проход: SolidWorks не зовут, спорное помечено.
    let quick = app
        .intake_glance(&source, ShapeCheck::Names, &NoProgress)
        .unwrap();
    assert!(quick.needs_refine());
    let plan = quick.plan(&[]);
    let item = |plan: &nonius_app::IntakePlan, name: &str| {
        plan.items.iter().find(|i| i.name == name).cloned().unwrap()
    };
    assert_eq!(item(&plan, "вал").shape, Some(ShapeVerdict::Different));
    assert!(item(&plan, "вал").doubtful && item(&plan, "вал_v2").doubtful);
    assert!(!item(&plan, "ось").doubtful);
    assert_eq!(item(&plan, "ось").shape, Some(ShapeVerdict::Confirmed));
    assert!(converted.lock().unwrap().is_empty());

    // Второй проход: точная форма — у спорного вала, обе версии; слова
    // «что изменилось» — у всех пар. Каждый файл SolidWorks переводит раз.
    let exact = app.intake_refine(&quick, &NoProgress);
    assert!(!exact.needs_refine());
    let plan = exact.plan(&[]);
    let shaft = item(&plan, "вал");
    assert_eq!(shaft.versions.len(), 2);
    assert_eq!(shaft.shape, Some(ShapeVerdict::Confirmed));
    assert!(!shaft.doubtful);
    let mut asked = converted.lock().unwrap().clone();
    asked.sort();
    assert_eq!(
        asked,
        ["вал.sldprt", "вал_v2.sldprt", "ось.sldprt", "ось_v2.sldprt"]
    );

    // Следующий разбор помнит точную форму: сразу верный ответ.
    let again = app
        .intake_glance(&source, ShapeCheck::Names, &NoProgress)
        .unwrap();
    let plan = again.plan(&[]);
    assert_eq!(item(&plan, "вал").shape, Some(ShapeVerdict::Confirmed));
    assert!(!item(&plan, "вал").doubtful);
    assert!(!again.needs_refine(), "всё уже известно");
    assert_eq!(converted.lock().unwrap().len(), 4, "SolidWorks не звали");
}
