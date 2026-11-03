//! Проверка папки проекта на диске.

use std::path::Path;

use nonius_core::meta::ItemKind;
use nonius_core::project::{Member, Role, META_FILE};
use nonius_core::{Error, PartName, Project};

#[test]
fn init_creates_layout_and_reopens() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("станок");
    let p = Project::init(&root, "Станок ЭЭО").unwrap();

    for f in ["project.yaml", "AGENTS.md", ".gitattributes", "notes"] {
        assert!(root.join(f).exists(), "нет {f}");
    }
    assert!(!root.join("parts").exists() && !root.join("assemblies").exists());
    assert!(!p.is_old_layout());
    assert_eq!(p.config().name, "Станок ЭЭО");
    assert!((p.config().compare.tolerance_mm - 0.01).abs() < 1e-12);

    let again = Project::open(&root).unwrap();
    assert_eq!(again.config(), p.config());
    assert!(matches!(
        Project::init(&root, "x"),
        Err(Error::AlreadyExists(_))
    ));
}

#[test]
fn items_are_created_listed_and_unique() {
    let tmp = tempfile::tempdir().unwrap();
    let p = Project::init(tmp.path(), "test").unwrap();
    let bracket = PartName::new("кронштейн").unwrap();
    let shaft = PartName::new("вал").unwrap();
    let frame = PartName::new("рама").unwrap();

    let item = p.create_item(ItemKind::Part, &bracket).unwrap();
    assert!(item.dir.join(META_FILE).is_file());
    assert!(item.dir.join("docs").is_dir());
    p.create_item(ItemKind::Part, &shaft).unwrap();
    p.create_item(ItemKind::Assembly, &frame).unwrap();

    let names: Vec<_> = p
        .items()
        .unwrap()
        .into_iter()
        .map(|i| i.name.to_string())
        .collect();
    assert_eq!(names, ["вал", "кронштейн", "рама"]);

    // имена уникальны во всём проекте, даже между деталями и сборками
    assert!(p.create_item(ItemKind::Assembly, &bracket).is_err());

    let meta = p.load_meta(&item).unwrap();
    assert_eq!(meta.kind, ItemKind::Part);
}

#[test]
fn discover_walks_up_and_config_updates() {
    let tmp = tempfile::tempdir().unwrap();
    let mut p = Project::init(tmp.path(), "test").unwrap();
    let deep = tmp.path().join("Стена").join("вал").join("docs");
    std::fs::create_dir_all(&deep).unwrap();
    assert_eq!(Project::discover(&deep).unwrap().root(), tmp.path());

    p.update_config(|c| {
        c.members.push(Member {
            name: "Vladimir".into(),
            role: Role::Leader,
        })
    })
    .unwrap();
    let reopened = Project::open(tmp.path()).unwrap();
    assert_eq!(reopened.config().members[0].role, Role::Leader);

    let empty = tempfile::tempdir().unwrap();
    assert!(matches!(
        Project::discover(empty.path()),
        Err(Error::NotAProject(_))
    ));
}

#[test]
fn items_live_in_any_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let p = Project::init(tmp.path(), "test").unwrap();
    let rail = PartName::new("RAIL-01").unwrap();
    p.create_folder(Path::new("Стена/Рейка")).unwrap();
    assert!(tmp.path().join("Стена/Рейка/.gitkeep").is_file());
    let item = p
        .create_item_in(ItemKind::Assembly, &rail, Path::new("Стена/Рейка"))
        .unwrap();
    assert_eq!(item.dir, tmp.path().join("Стена/Рейка/RAIL-01"));
    p.create_item(ItemKind::Part, &PartName::new("PV-01").unwrap())
        .unwrap();

    let layout = p.scan().unwrap();
    assert_eq!(layout.folders, ["notes", "Стена", "Стена/Рейка"]);
    let items = p.items().unwrap();
    assert_eq!(items.len(), 2);
    let found = p.find_item(&rail).unwrap().unwrap();
    assert_eq!(found.kind, ItemKind::Assembly, "вид — из meta.yaml");

    // Внутрь детали не кладём, из проекта не выходим, имена уникальны везде.
    assert!(matches!(
        p.create_folder(Path::new("Стена/Рейка/RAIL-01/x")),
        Err(Error::InsideItem(_))
    ));
    assert!(p.create_folder(Path::new("../снаружи")).is_err());
    assert!(matches!(
        p.create_item_in(ItemKind::Part, &rail, Path::new("")),
        Err(Error::AlreadyExists(_))
    ));
}

#[test]
fn old_layout_opens_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("project.yaml"),
        "schema: 1
name: Старый
",
    )
    .unwrap();
    let dir = tmp.path().join("parts").join("вал");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(META_FILE),
        "schema: 1
kind: part
",
    )
    .unwrap();

    let mut p = Project::open(tmp.path()).unwrap();
    assert!(p.is_old_layout());
    assert_eq!(p.items().unwrap()[0].dir, dir, "старые детали видны");
    let name = PartName::new("ось").unwrap();
    assert!(matches!(
        p.create_item(ItemKind::Part, &name),
        Err(Error::OldLayout(_))
    ));
    assert!(p.update_config(|c| c.name = "x".into()).is_err());

    p.finish_upgrade().unwrap();
    let text = std::fs::read_to_string(tmp.path().join("project.yaml")).unwrap();
    assert!(
        text.starts_with(
            "schema: 2
"
        ),
        "{text}"
    );
    assert!(!Project::open(tmp.path()).unwrap().is_old_layout());
}
