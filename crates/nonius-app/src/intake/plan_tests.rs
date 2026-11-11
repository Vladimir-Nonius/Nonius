//! Тесты правил разбора на придуманных папках.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use nonius_core::meta::ItemKind;
use nonius_core::PartName;

use super::super::{key, Found, IntakePlan};
use super::build;

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
        path,
    }
}

fn link(links: &mut HashMap<String, Vec<String>>, from: &str, to: &[&str]) {
    let full = |rel: &str| key(&Path::new(ROOT).join(rel));
    links.insert(full(from), to.iter().map(|t| full(t)).collect());
}

fn plan(files: &[Found], links: &HashMap<String, Vec<String>>) -> IntakePlan {
    build(PathBuf::from(ROOT), files.to_vec(), links, &[])
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
    let p = build(PathBuf::from(ROOT), files.to_vec(), &HashMap::new(), &taken);
    assert_eq!(names(&p), ["втулка"]);
    assert!(p.left[0].why.contains("уже есть"), "{:?}", p.left);
}
