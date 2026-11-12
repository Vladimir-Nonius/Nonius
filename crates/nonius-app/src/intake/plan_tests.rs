//! Тесты правил разбора на придуманных папках.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use nonius_core::meta::ItemKind;
use nonius_core::PartName;

use super::super::questions::Decisions;
use super::super::shape::Shape;
use super::super::words::Words;
use super::super::{key, Found, IntakePlan, ShapeVerdict};
use super::{build, Facts};

const ROOT: &str = "/папка";

/// Файл `rel` (путь внутри папки), изменённый в «минуту» `at`.
fn file(rel: &str, at: u64) -> Found {
    let path = Path::new(ROOT).join(rel);
    let dir = rel.rsplit_once('/').map_or("", |(d, _)| d).to_string();
    Found {
        stem: path.file_stem().unwrap().to_string_lossy().into_owned(),
        ext: path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default(),
        dir,
        modified: UNIX_EPOCH + Duration::from_secs(at * 60),
        digest: None,
        path,
    }
}

fn link(links: &mut HashMap<String, Vec<String>>, from: &str, to: &[&str]) {
    let full = |rel: &str| key(&Path::new(ROOT).join(rel));
    links.insert(full(from), to.iter().map(|t| full(t)).collect());
}

fn plan(files: &[Found], links: &HashMap<String, Vec<String>>) -> IntakePlan {
    build(
        PathBuf::from(ROOT),
        files.to_vec(),
        Facts {
            links,
            taken: &[],
            shapes: &HashMap::new(),
            said: &Decisions::default(),
            across: false,
            words: &Words::default(),
        },
    )
}

fn none_shapes() -> HashMap<String, Shape> {
    HashMap::new()
}

/// Форма файла `rel`: объём и габариты.
fn shaped(shapes: &mut HashMap<String, Shape>, rel: &str, volume: f64, dims: [f64; 3]) {
    let shape = Shape {
        volume,
        area: None,
        dims,
        faces: None,
        rough: false,
    };
    shapes.insert(key(&Path::new(ROOT).join(rel)), shape);
}

/// Имена деталей плана с папками: `папка/имя`.
fn names(plan: &IntakePlan) -> Vec<String> {
    plan.items
        .iter()
        .map(|i| {
            if i.folder.is_empty() {
                i.name.clone()
            } else {
                format!("{}/{}", i.folder, i.name)
            }
        })
        .collect()
}

fn labels(plan: &IntakePlan, name: &str) -> Vec<String> {
    let item = plan.items.iter().find(|i| i.name == name).unwrap();
    item.versions.iter().map(|v| v.label.clone()).collect()
}

#[test]
fn one_name_is_one_part() {
    let files = [
        file("вал.SLDPRT", 1),
        file("вал.step", 1),
        file("вал.pdf", 1),
        file("вал.SLDDRW", 1),
        file("заметки.pdf", 1),
        file("расчёт.log", 1),
    ];
    let p = plan(&files, &HashMap::new());
    assert_eq!(names(&p), ["вал"]);
    let item = &p.items[0];
    assert_eq!(item.kind, ItemKind::Part);
    assert_eq!(item.versions.len(), 1);
    // STEP идёт первым: свежий STEP из SolidWorks потом его заменит.
    let order: Vec<String> = item.versions[0]
        .models
        .iter()
        .map(|m| m.extension().unwrap().to_string_lossy().to_lowercase())
        .collect();
    assert_eq!(order, ["step", "sldprt"]);
    assert_eq!(item.docs.len(), 2, "чертёж и pdf");
    assert_eq!(p.left.len(), 2, "заметки и лог не привязаны ни к чему");
    assert!(p.left[0].why.contains("не понял"));
    assert_eq!(p.left[1].why, super::super::NOT_OURS);
}

#[test]
fn names_with_marks_are_versions() {
    let files = [
        file("вал_v2.SLDPRT", 5),
        file("вал.SLDPRT", 9),
        file("вал (3).SLDPRT", 1),
        file("втулка.SLDPRT", 1),
    ];
    let p = plan(&files, &HashMap::new());
    assert_eq!(names(&p), ["вал", "втулка"]);
    // По номерам, а не по времени: файл без номера — первый.
    assert_eq!(labels(&p, "вал"), ["вал", "вал_v2", "вал (3)"]);
    assert!(p.items[0].guessed && !p.items[1].guessed);
    assert!(
        p.items[0].why[0].contains("3 версии"),
        "{:?}",
        p.items[0].why
    );
}

#[test]
fn words_are_ordered_by_time() {
    let files = [
        file("крышка доработаная.SLDPRT", 7),
        file("крышка.SLDPRT", 2),
        file("крышка - копия.SLDPRT", 4),
    ];
    let p = plan(&files, &HashMap::new());
    assert_eq!(
        labels(&p, "крышка"),
        ["крышка", "крышка - копия", "крышка доработаная"]
    );
}

#[test]
fn numbered_parts_are_not_versions() {
    let files = [
        file("ЗАГЛУШКА1.SLDPRT", 1),
        file("ЗАГЛУШКА2.SLDPRT", 2),
        file("ЗАГЛУШКА12.SLDPRT", 3),
    ];
    let p = plan(&files, &HashMap::new());
    assert_eq!(names(&p), ["ЗАГЛУШКА1", "ЗАГЛУШКА12", "ЗАГЛУШКА2"]);
}

#[test]
fn parts_of_one_assembly_are_different_parts() {
    // «кронштейн» и «кронштейн (2)» стоят в одной сборке: это две детали.
    let files = [
        file("узел.SLDASM", 1),
        file("кронштейн.SLDPRT", 1),
        file("кронштейн (2).SLDPRT", 2),
    ];
    let mut links = HashMap::new();
    link(
        &mut links,
        "узел.SLDASM",
        &["кронштейн.SLDPRT", "кронштейн (2).SLDPRT"],
    );
    let p = plan(&files, &links);
    assert_eq!(names(&p), ["кронштейн", "кронштейн (2)", "узел"]);
    assert_eq!(p.items[2].kind, ItemKind::Assembly);
}

#[test]
fn drawing_goes_to_the_model_it_refers_to() {
    let files = [
        file("фланец.SLDPRT", 1),
        file("чертеж фланца.SLDDRW", 1),
        file("сирота.SLDDRW", 1),
    ];
    let mut links = HashMap::new();
    link(&mut links, "чертеж фланца.SLDDRW", &["фланец.SLDPRT"]);
    let p = plan(&files, &links);
    assert_eq!(names(&p), ["фланец"]);
    assert_eq!(p.items[0].docs.len(), 1);
    assert!(
        p.items[0].why[0].contains("ссылается"),
        "{:?}",
        p.items[0].why
    );
    assert_eq!(p.left.len(), 1);
    assert!(p.left[0].why.contains("не нашёл"));
}

#[test]
fn several_assemblies_get_their_own_folders() {
    let files = [
        file("насос.SLDASM", 1),
        file("клапан.SLDASM", 1),
        file("корпус.SLDPRT", 1),
        file("шток.SLDPRT", 1),
        file("болт.SLDPRT", 1),
        file("лишняя.SLDPRT", 1),
    ];
    let mut links = HashMap::new();
    link(
        &mut links,
        "насос.SLDASM",
        &["корпус.SLDPRT", "болт.SLDPRT"],
    );
    link(&mut links, "клапан.SLDASM", &["шток.SLDPRT", "болт.SLDPRT"]);
    let p = plan(&files, &links);
    assert_eq!(
        names(&p),
        [
            "болт",
            "лишняя",
            "клапан/клапан",
            "клапан/шток",
            "насос/корпус",
            "насос/насос",
        ]
    );
    let bolt = &p.items[0];
    assert!(bolt.why[0].contains("в 2 сборках"), "{:?}", bolt.why);
}

#[test]
fn one_assembly_keeps_the_folder_as_it_is() {
    let files = [
        file("кардан/Сборка1.SLDASM", 1),
        file("кардан/вилка.SLDPRT", 1),
        file("Сборка1.SLDASM", 1),
    ];
    let mut links = HashMap::new();
    link(
        &mut links,
        "кардан/Сборка1.SLDASM",
        &["кардан/вилка.SLDPRT"],
    );
    let p = plan(&files, &links);
    // Одинаковые имена из разных папок различаются именем папки.
    assert_eq!(
        names(&p),
        ["Сборка1", "кардан/Сборка1 (кардан)", "кардан/вилка"]
    );
}

#[test]
fn names_taken_in_the_project_stay_out() {
    let files = [file("вал.SLDPRT", 1), file("втулка.SLDPRT", 1)];
    let taken = [PartName::new("Вал").unwrap()];
    let none = HashMap::new();
    let p = build(
        PathBuf::from(ROOT),
        files.to_vec(),
        Facts {
            links: &none,
            taken: &taken,
            shapes: &none_shapes(),
            said: &Decisions::default(),
            across: false,
            words: &Words::default(),
        },
    );
    assert_eq!(names(&p), ["втулка"]);
    assert!(p.left[0].why.contains("уже есть"), "{:?}", p.left);
}

#[test]
fn shape_confirms_or_splits_versions() {
    let files = [
        file("вал.SLDPRT", 1),
        file("вал_v2.SLDPRT", 2),
        file("кожух.SLDPRT", 1),
        file("кожух_v2.SLDPRT", 2),
        file("люк.SLDPRT", 1),
    ];
    let mut shapes = none_shapes();
    shaped(&mut shapes, "вал.SLDPRT", 10_000.0, [20.0, 20.0, 100.0]);
    shaped(&mut shapes, "вал_v2.SLDPRT", 10_400.0, [20.0, 20.0, 100.0]);
    shaped(&mut shapes, "кожух.SLDPRT", 5_000.0, [2.0, 80.0, 120.0]);
    shaped(&mut shapes, "кожух_v2.SLDPRT", 900.0, [10.0, 10.0, 30.0]);
    // «Люк» назван иначе, а формой — почти «кожух».
    shaped(&mut shapes, "люк.SLDPRT", 5_100.0, [2.0, 80.0, 121.0]);
    let answered = |answers: &[(&str, bool)]| {
        let answers: Vec<(String, bool)> = answers
            .iter()
            .map(|(id, yes)| (id.to_string(), *yes))
            .collect();
        build(
            PathBuf::from(ROOT),
            files.to_vec(),
            Facts {
                links: &HashMap::new(),
                taken: &[],
                shapes: &shapes,
                said: &Decisions::new(&answers),
                across: false,
                words: &Words::default(),
            },
        )
    };
    let p = answered(&[]);
    assert_eq!(names(&p), ["вал", "кожух", "кожух_v2", "люк"]);
    let item = |name: &str| p.items.iter().find(|i| i.name == name).unwrap();

    let shaft = item("вал");
    assert_eq!(labels(&p, "вал"), ["вал", "вал_v2"]);
    assert_eq!(shaft.shape, Some(ShapeVerdict::Confirmed));
    assert!(shaft.why.iter().any(|w| w.contains("подтверждено формой")));

    // Разная форма: две детали, и обе это объясняют.
    let (cover, other) = (item("кожух"), item("кожух_v2"));
    assert_eq!(cover.versions.len(), 1);
    assert_eq!(cover.shape, Some(ShapeVerdict::Different));
    assert!(!cover.guessed && !other.guessed);
    assert!(other.why[0].starts_with("имя похоже на «кожух», но форма другая"));

    // Похожие формой детали с разными именами не объединяются: подсказка.
    assert!(cover.why.iter().any(|w| w.contains("похожа на «люк»")));
    assert!(item("люк").why[0].contains("похожа на «кожух»"));
    assert_eq!(item("люк").shape, None);

    // Вопросы: про разведённый кожух (программа думает «нет») и про люк.
    let ids: Vec<&str> = p.questions.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, ["кожух|кожух_v2", "кожух|люк"]);
    assert_eq!(p.questions[0].suggested, Some(false));
    assert_eq!(
        p.questions[0].text,
        "Это версии одной детали: «кожух» → «кожух_v2»?"
    );
    assert!(p.questions[1].facts[0].contains("форма похожа"));

    // Ответы: кожух_v2 — всё-таки версия; люк — версия кожуха.
    let p = answered(&[("кожух|кожух_v2", true), ("кожух|люк", true)]);
    assert_eq!(names(&p), ["вал", "кожух"]);
    assert_eq!(labels(&p, "кожух"), ["кожух", "люк", "кожух_v2"]);
    assert!(p.questions.is_empty(), "{:?}", p.questions);
    assert!(!p.items[1].guessed);

    // «Нет» на оба: всё остаётся врозь, и больше не спрашивают.
    let p = answered(&[("кожух|кожух_v2", false), ("кожух|люк", false)]);
    assert_eq!(names(&p), ["вал", "кожух", "кожух_v2", "люк"]);
    assert!(p.questions.is_empty(), "{:?}", p.questions);
    assert!(
        p.items[3].why.is_empty(),
        "подсказка снята: {:?}",
        p.items[3].why
    );
}

#[test]
fn unchecked_guess_becomes_a_question() {
    let files = [file("вал.SLDPRT", 1), file("вал_v2.SLDPRT", 2)];
    let p = plan(&files, &HashMap::new());
    assert_eq!(p.questions.len(), 1);
    assert_eq!(p.questions[0].id, "вал|вал_v2");
    assert_eq!(p.questions[0].suggested, None);
    assert!(p.questions[0].facts[0].contains("только по именам"));
    let no = Decisions::new(&[("вал|вал_v2".into(), false)]);
    let p = build(
        PathBuf::from(ROOT),
        files.to_vec(),
        Facts {
            links: &HashMap::new(),
            taken: &[],
            shapes: &none_shapes(),
            said: &no,
            across: false,
            words: &Words::default(),
        },
    );
    assert_eq!(names(&p), ["вал", "вал_v2"]);
    assert!(p.questions.is_empty());
}

#[test]
fn project_copied_by_folders_is_one_project() {
    // Проект скопировали папкой: «рабочая» → «готовая», и в копии работали.
    let copy = |rel: &str, at: u64| Found {
        digest: Some("тот же файл".into()),
        ..file(rel, at)
    };
    let files = [
        copy("рабочая/FK12.SLDPRT", 1),
        copy("готовая/FK12.SLDPRT", 1),
        file("рабочая/вал.SLDPRT", 1),
        file("готовая/вал.SLDPRT", 5),
        file("рабочая/кронштейн.SLDPRT", 1),
        file("готовая/кронштейн.SLDPRT", 5),
        file("рабочая/плита.SLDPRT", 1),
        file("готовая/плита.SLDPRT", 5),
    ];
    let mut shapes = none_shapes();
    shaped(
        &mut shapes,
        "рабочая/вал.SLDPRT",
        10_000.0,
        [20.0, 20.0, 100.0],
    );
    shaped(
        &mut shapes,
        "готовая/вал.SLDPRT",
        10_400.0,
        [20.0, 20.0, 100.0],
    );
    shaped(
        &mut shapes,
        "рабочая/кронштейн.SLDPRT",
        5_000.0,
        [2.0, 80.0, 120.0],
    );
    shaped(
        &mut shapes,
        "готовая/кронштейн.SLDPRT",
        900.0,
        [10.0, 10.0, 30.0],
    );
    let built = |shapes: &HashMap<String, Shape>, across: bool| {
        build(
            PathBuf::from(ROOT),
            files.to_vec(),
            Facts {
                links: &HashMap::new(),
                taken: &[],
                shapes,
                said: &Decisions::default(),
                across,
                words: &Words::default(),
            },
        )
    };
    let p = built(&shapes, false);
    assert_eq!(
        names(&p),
        [
            "готовая/FK12",
            "готовая/вал",
            "готовая/кронштейн",
            "готовая/плита",
            "рабочая/кронштейн (рабочая)",
            "рабочая/плита (рабочая)",
        ]
    );
    let item = |name: &str| p.items.iter().find(|i| i.name == name).unwrap();

    // Одинаковый файл — одна деталь, одна версия, в «живой» папке.
    assert_eq!(item("FK12").versions.len(), 1);
    assert_eq!(
        item("FK12").why,
        ["та же деталь лежит ещё тут: папка «рабочая»"]
    );
    assert!(!item("FK12").guessed);

    // Форма похожа — версии; подписи различают папки.
    assert_eq!(
        labels(&p, "вал"),
        ["вал (папка «рабочая»)", "вал (папка «готовая»)"]
    );
    assert_eq!(item("вал").shape, Some(ShapeVerdict::Confirmed));

    // Форма разная — разные детали; про тёзок из разных папок не спрашиваем.
    assert_eq!(item("кронштейн").shape, Some(ShapeVerdict::Different));
    // Формы нет — одному имени в разных папках не верим.
    assert_eq!(item("плита").versions.len(), 1);
    assert!(p.questions.is_empty(), "{:?}", p.questions);

    // Черновик верит именам: по нему видно, чью форму смотреть.
    let draft = built(&none_shapes(), true);
    let guessed: Vec<&str> = draft
        .items
        .iter()
        .filter(|i| i.guessed)
        .map(|i| i.name.as_str())
        .collect();
    assert_eq!(guessed, ["вал", "кронштейн", "плита"]);
}
