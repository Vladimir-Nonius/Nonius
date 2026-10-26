//! Проверка папки проекта на диске.

use nonius_core::meta::ItemKind;
use nonius_core::project::{Member, Role, META_FILE};
use nonius_core::{Error, PartName, Project};

#[test]
fn init_creates_layout_and_reopens() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("станок");
    let p = Project::init(&root, "Станок ЭЭО").unwrap();

    for f in [
        "project.yaml",
        "AGENTS.md",
        ".gitattributes",
        "parts",
        "assemblies",
        "notes",
    ] {
        assert!(root.join(f).exists(), "нет {f}");
    }
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
    let deep = tmp.path().join("parts").join("вал").join("docs");
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
