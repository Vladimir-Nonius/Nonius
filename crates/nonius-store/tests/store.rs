//! Хранилище версий на настоящем git во временной папке.

use std::fs;

use nonius_core::meta::ItemKind;
use nonius_core::project::META_FILE;
use nonius_core::{schema, PartName, Project, VersionNumber};
use nonius_store::{FileChangeKind, GitOptions, Store, StoreError};

fn options() -> GitOptions {
    GitOptions {
        identity: Some(("Тестировщик".into(), "test@example.com".into())),
    }
}

fn v(n: u32) -> VersionNumber {
    VersionNumber::new(n).unwrap()
}

fn setup(part: &str) -> (tempfile::TempDir, Store, nonius_core::project::Item) {
    let tmp = tempfile::tempdir().unwrap();
    let project = Project::init(tmp.path(), "Тест").unwrap();
    let item = project
        .create_item(ItemKind::Part, &PartName::new(part).unwrap())
        .unwrap();
    let store = Store::open(project, &options()).unwrap();
    (tmp, store, item)
}

fn set_mass(item: &nonius_core::project::Item, store: &Store, mass: f64) {
    let mut meta = store.project().load_meta(item).unwrap();
    meta.geometry.mass_kg = Some(mass);
    schema::save(&meta, &item.dir.join(META_FILE)).unwrap();
}

#[test]
fn commit_log_and_read_back() {
    let (_tmp, store, item) = setup("вал 20 мм");

    assert!(store.versions(&item).unwrap().is_empty());
    let changes = store.changes(&item).unwrap();
    assert!(changes
        .iter()
        .any(|c| c.path == "meta.yaml" && c.kind == FileChangeKind::Added));

    fs::write(
        item.dir.join("model.step"),
        b"ISO-10303-21;\nEND-ISO-10303-21;\n",
    )
    .unwrap();
    let v1 = store.commit(&item, "Первая версия\n\nПо эскизу.").unwrap();
    assert_eq!(v1.version, v(1));
    assert_eq!(v1.author, "Тестировщик");
    assert_eq!(v1.message, "Первая версия\n\nПо эскизу.");
    assert!(store.changes(&item).unwrap().is_empty());

    assert!(matches!(
        store.commit(&item, "ещё раз"),
        Err(StoreError::NoChanges(_))
    ));
    assert!(matches!(
        store.commit(&item, "  "),
        Err(StoreError::EmptyMessage)
    ));

    set_mass(&item, &store, 1.5);
    let v2 = store.commit(&item, "Масса").unwrap();
    assert_eq!(v2.version, v(2));

    let log = store.versions(&item).unwrap();
    assert_eq!(
        log.iter().map(|x| x.version).collect::<Vec<_>>(),
        [v(2), v(1)]
    );

    assert_eq!(store.meta_at(&item, &v1).unwrap().geometry.mass_kg, None);
    assert_eq!(
        store.meta_at(&item, &v2).unwrap().geometry.mass_kg,
        Some(1.5)
    );
    assert_eq!(
        store.read_file(&item, &v1, "model.step").unwrap(),
        b"ISO-10303-21;\nEND-ISO-10303-21;\n"
    );
    let mut files = store.files_at(&item, &v2).unwrap();
    files.sort();
    assert_eq!(files, ["meta.yaml", "model.step"]);
}

#[test]
fn versions_are_per_item() {
    let (_tmp, store, a) = setup("кронштейн");
    let b = store
        .project()
        .create_item(ItemKind::Part, &PartName::new("рама").unwrap())
        .unwrap();
    store.commit(&a, "кронштейн готов").unwrap();
    store.commit(&b, "рама готова").unwrap();
    set_mass(&a, &store, 2.0);
    store.commit(&a, "тяжелее").unwrap();

    assert_eq!(store.versions(&a).unwrap().len(), 2);
    assert_eq!(store.versions(&b).unwrap().len(), 1);
    assert_eq!(store.latest(&b).unwrap().unwrap().version, v(1));
    assert_eq!(store.item("рама").unwrap().name, b.name);
    assert!(matches!(
        store.item("нет такой"),
        Err(StoreError::UnknownItem(_))
    ));
}

#[test]
fn restore_creates_new_version_and_keeps_history() {
    let (_tmp, store, item) = setup("кронштейн");
    fs::write(item.dir.join("model.step"), b"v1").unwrap();
    store.commit(&item, "v1").unwrap();

    fs::write(item.dir.join("model.step"), b"v2").unwrap();
    fs::write(item.dir.join("лишний.txt"), b"x").unwrap();
    store.commit(&item, "v2").unwrap();

    fs::write(item.dir.join("model.step"), b"dirty").unwrap();
    assert!(matches!(
        store.restore(&item, v(1), None),
        Err(StoreError::Dirty(_))
    ));
    fs::write(item.dir.join("model.step"), b"v2").unwrap();

    let v3 = store.restore(&item, v(1), Some("v2 не подошла")).unwrap();
    assert_eq!(v3.version, v(3));
    assert!(v3.summary().starts_with("Откат к v1: v2 не подошла"));
    assert_eq!(fs::read(item.dir.join("model.step")).unwrap(), b"v1");
    assert!(!item.dir.join("лишний.txt").exists());
    assert!(item.dir.join("docs").is_dir());
    assert_eq!(
        store.versions(&item).unwrap().len(),
        3,
        "история не теряется"
    );

    assert!(matches!(
        store.restore(&item, v(9), None),
        Err(StoreError::UnknownVersion { .. })
    ));
}

#[test]
fn export_writes_version_files() {
    let (tmp, store, item) = setup("кронштейн");
    fs::create_dir_all(item.dir.join("docs")).unwrap();
    fs::write(item.dir.join("docs").join("чертёж.pdf"), b"%PDF-1.4").unwrap();
    let v1 = store.commit(&item, "с чертежом").unwrap();

    let dest = tmp.path().join("выгрузка");
    let files = store.export(&item, &v1, &dest).unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(
        fs::read(dest.join("docs").join("чертёж.pdf")).unwrap(),
        b"%PDF-1.4"
    );
}

#[test]
fn reopening_existing_repository_keeps_history() {
    let (tmp, store, item) = setup("кронштейн");
    store.commit(&item, "первая").unwrap();
    drop(store);
    let again = Store::open(Project::open(tmp.path()).unwrap(), &options()).unwrap();
    assert_eq!(again.versions(&item).unwrap().len(), 1);
}

#[test]
fn snapshot_points_to_files_of_that_version() {
    let (_tmp, store, item) = setup("кронштейн");
    fs::write(item.dir.join("model.step"), b"first").unwrap();
    let v1 = store.commit(&item, "v1").unwrap();
    fs::write(item.dir.join("model.step"), b"second").unwrap();
    store.commit(&item, "v2").unwrap();

    let snap = store.snapshot(&item, &v1).unwrap();
    assert_eq!(snap.version, Some(v(1)));
    assert_ne!(snap.dir, item.dir, "старая версия — не рабочая папка");
    assert_eq!(fs::read(snap.dir.join("model.step")).unwrap(), b"first");
    // повторный вызов берёт кэш
    assert_eq!(store.snapshot(&item, &v1).unwrap().dir, snap.dir);
    // кэш не попадает в изменения детали
    assert!(store.changes(&item).unwrap().is_empty());
}
