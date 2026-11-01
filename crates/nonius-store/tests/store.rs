//! Хранилище версий на настоящем git во временной папке.

use std::fs;

use nonius_core::meta::ItemKind;
use nonius_core::project::META_FILE;
use nonius_core::{schema, PartName, Project, VersionNumber};
use nonius_store::{AiActor, DiffLineKind, FileChangeKind, GitOptions, Store, StoreError};

fn options() -> GitOptions {
    GitOptions {
        identity: Some(("Тестировщик".into(), "test@example.com".into())),
        ai: None,
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
fn histories_match_item_by_item_queries() {
    let (tmp, store, a) = setup("кронштейн");
    let b = store
        .project()
        .create_item(ItemKind::Part, &PartName::new("вал 20").unwrap())
        .unwrap();
    let c = store
        .project()
        .create_item(ItemKind::Assembly, &PartName::new("рама").unwrap())
        .unwrap();
    store.commit(&a, "первая").unwrap();
    store.commit(&b, "вал").unwrap();
    set_mass(&a, &store, 1.5);
    store.commit(&a, "вторая").unwrap();
    set_mass(&b, &store, 2.5);
    std::fs::write(b.dir.join("docs").join("новый.txt"), "текст").unwrap();
    std::fs::write(tmp.path().join("вне деталей.txt"), "текст").unwrap();

    let items = [a, b, c];
    let histories = store.histories(&items).unwrap();
    for (item, history) in items.iter().zip(&histories) {
        assert_eq!(history.versions, store.versions(item).unwrap());
        assert_eq!(history.changes, store.changes(item).unwrap());
    }
    assert_eq!(histories[0].versions.len(), 2);
    assert_eq!(histories[1].changes.len(), 2);
    assert!(histories[2].versions.is_empty());
    assert!(!histories[2].changes.is_empty(), "новая сборка");
}

#[test]
fn ai_is_the_author_and_person_stays_committer() {
    let (tmp, store, item) = setup("кронштейн");
    store.commit(&item, "человек").unwrap();
    assert_eq!(store.latest(&item).unwrap().unwrap().ai, None);

    let ai = AiActor::new("Claude", Some("claude-opus-5-5")).unwrap();
    let mut with_ai = options();
    with_ai.ai = Some(ai.clone());
    let project = Project::discover(tmp.path()).unwrap();
    let store = Store::open(project, &with_ai).unwrap();
    set_mass(&item, &store, 1.5);
    let saved = store.commit(&item, "ИИ").unwrap();
    assert_eq!(saved.author, "Claude");
    assert_eq!(saved.committer, "Тестировщик");
    assert_eq!(saved.ai, Some(ai.clone()));

    // Откат тоже подписан.
    let back = store.restore(&item, v(1), None).unwrap();
    assert_eq!(back.ai, Some(ai));
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

#[test]
fn files_between_versions() {
    let (_tmp, store, item) = setup("кронштейн");
    let docs = item.dir.join("docs");
    fs::create_dir_all(&docs).unwrap();
    fs::write(docs.join("чертёж.pdf"), b"%PDF-1.4").unwrap();
    fs::write(docs.join("заметка.md"), "строка 1\nстрока 2\n").unwrap();
    fs::write(docs.join("старый.txt"), "уйдёт\n").unwrap();
    let big = vec![b'x'; 5000];
    fs::write(item.dir.join("model.step"), &big).unwrap();
    let v1 = store.commit(&item, "первая").unwrap();

    fs::write(docs.join("чертёж.pdf"), b"%PDF-1.5 second").unwrap();
    fs::write(docs.join("заметка.md"), "строка 1\nстрока 3\n").unwrap();
    fs::remove_file(docs.join("старый.txt")).unwrap();
    fs::write(docs.join("новый.dxf"), b"0\nEOF\n").unwrap();
    let v2 = store.commit(&item, "вторая").unwrap();

    let list = store.files_between(&item, Some(&v1), Some(&v2)).unwrap();
    let find = |list: &[nonius_store::VersionFile], p: &str| {
        list.iter().find(|f| f.path == p).unwrap().clone()
    };
    let pdf = find(&list, "docs/чертёж.pdf");
    assert_eq!(pdf.change, Some(FileChangeKind::Modified));
    assert_eq!(
        (pdf.before, pdf.after),
        (Some(8), Some(15)),
        "настоящий размер, не указатель LFS"
    );
    assert_eq!(
        find(&list, "docs/новый.dxf").change,
        Some(FileChangeKind::Added)
    );
    let gone = find(&list, "docs/старый.txt");
    assert_eq!(gone.change, Some(FileChangeKind::Deleted));
    assert_eq!(gone.after, None);
    let model = find(&list, "model.step");
    assert_eq!(model.change, None);
    assert_eq!(model.after, Some(5000));

    // Без «было» — просто файлы версии.
    let plain = store.files_between(&item, None, Some(&v1)).unwrap();
    assert!(plain.iter().all(|f| f.change.is_none()));
    assert!(plain.iter().any(|f| f.path == "docs/старый.txt"));

    // Разница текста.
    let lines = store
        .text_diff(&item, Some(&v1), Some(&v2), "docs/заметка.md")
        .unwrap();
    let got: Vec<_> = lines.iter().map(|l| (l.kind, l.text.as_str())).collect();
    assert!(
        got.contains(&(DiffLineKind::Removed, "строка 2")),
        "{got:?}"
    );
    assert!(got.contains(&(DiffLineKind::Added, "строка 3")), "{got:?}");
    let whole = store
        .text_diff(&item, None, Some(&v1), "docs/заметка.md")
        .unwrap();
    assert_eq!(whole.len(), 2);

    // Сейчас: новый файл, который git ещё не видел, изменённый и удалённый.
    fs::write(docs.join("свежая.md"), "первая строка\n").unwrap();
    fs::write(docs.join("заметка.md"), "строка 1\nстрока 4\n").unwrap();
    fs::remove_file(docs.join("новый.dxf")).unwrap();
    let now = store.files_between(&item, Some(&v2), None).unwrap();
    let fresh = find(&now, "docs/свежая.md");
    assert_eq!(fresh.change, Some(FileChangeKind::Added));
    assert_eq!(fresh.before, None);
    assert_eq!(
        find(&now, "docs/заметка.md").change,
        Some(FileChangeKind::Modified)
    );
    let removed = find(&now, "docs/новый.dxf");
    assert_eq!(removed.change, Some(FileChangeKind::Deleted));
    assert_eq!(removed.after, None);
    let added = store
        .text_diff(&item, Some(&v2), None, "docs/свежая.md")
        .unwrap();
    assert_eq!(added[0].kind, DiffLineKind::Added);
    assert_eq!(added[0].text, "первая строка");
}
