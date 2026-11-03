//! Детали в папках: перенос, обновление старого проекта (`parts/`,
//! `assemblies/`), возврат удалённой — на настоящем git.

use std::fs;
use std::path::Path;
use std::process::Command;

use nonius_core::meta::ItemKind;
use nonius_core::project::Item;
use nonius_core::{Error as CoreError, PartName, Project, VersionNumber};
use nonius_store::{GitOptions, Store, StoreError};

fn options() -> GitOptions {
    GitOptions {
        identity: Some(("Тестировщик".into(), "test@example.com".into())),
        ai: None,
    }
}

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

fn v(n: u32) -> VersionNumber {
    VersionNumber::new(n).unwrap()
}

/// Две версии детали с разными `model.step`.
fn two_versions(store: &Store, item: &Item) {
    fs::write(item.dir.join("model.step"), "первая").unwrap();
    store.commit(item, "первая").unwrap();
    fs::write(item.dir.join("model.step"), "вторая").unwrap();
    store.commit(item, "вторая").unwrap();
}

fn read(store: &Store, item: &Item, n: u32) -> String {
    let version = store.version(item, v(n)).unwrap();
    String::from_utf8(store.read_file(item, &version, "model.step").unwrap()).unwrap()
}

fn git(root: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .current_dir(root)
        .args(["-c", "user.name=Т", "-c", "user.email=t@example.com"])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn moved_part_keeps_versions() {
    let tmp = tempfile::tempdir().unwrap();
    let project = Project::init(tmp.path(), "Тест").unwrap();
    let item = project.create_item(ItemKind::Part, &name("PV-01")).unwrap();
    let store = Store::open(project, &options()).unwrap();
    two_versions(&store, &item);

    let moved = store.move_item(&item, Path::new("Стена/Полки")).unwrap();
    assert_eq!(moved.dir, tmp.path().join("Стена/Полки/PV-01"));
    assert!(!item.dir.exists());
    assert!(
        store.changes(&moved).unwrap().is_empty(),
        "отметка всё взяла"
    );
    assert_eq!(read(&store, &moved, 1), "первая");

    // После переноса — переименование и новая версия: все версии на месте.
    let renamed = store.rename(&moved, &name("PV-02")).unwrap();
    assert_eq!(renamed.dir, tmp.path().join("Стена/Полки/PV-02"));
    fs::write(renamed.dir.join("model.step"), "третья").unwrap();
    store.commit(&renamed, "третья").unwrap();
    let back = store.move_item(&renamed, Path::new("")).unwrap();
    assert_eq!(store.versions(&back).unwrap().len(), 3);
    assert_eq!(read(&store, &back, 1), "первая");
    assert_eq!(read(&store, &back, 3), "третья");
    let all = store.histories(std::slice::from_ref(&back)).unwrap();
    assert_eq!(all[0].versions, store.versions(&back).unwrap());

    // Несохранённое с переносом не смешивается; внутрь детали не кладём.
    fs::write(back.dir.join("model.step"), "правка").unwrap();
    assert!(matches!(
        store.move_item(&back, Path::new("Стена")),
        Err(StoreError::Dirty(_))
    ));
    assert!(matches!(
        store.move_item(&back, Path::new("PV-02/внутри")),
        Err(StoreError::Core(CoreError::InsideItem(_)))
    ));
}

#[test]
fn deleted_part_comes_back_into_its_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let project = Project::init(tmp.path(), "Тест").unwrap();
    project.create_folder(Path::new("Стена")).unwrap();
    let item = project
        .create_item_in(ItemKind::Part, &name("PANEL-00"), Path::new("Стена"))
        .unwrap();
    let store = Store::open(project, &options()).unwrap();
    two_versions(&store, &item);

    store.delete(&item).unwrap();
    assert_eq!(
        store.deleted().unwrap()[0].folder.as_deref(),
        Some("Стена/PANEL-00")
    );
    let back = store.recover(&name("PANEL-00")).unwrap();
    assert_eq!(back.dir, item.dir);
    assert_eq!(read(&store, &back, 2), "вторая");
}

/// Проект старого устройства с историей: деталь `вал` в `parts/` с двумя
/// версиями, сборка `рама` в `assemblies/`, деталь `ось`, удалённая ещё до
/// перехода на папки.
fn old_project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let project = Project::init(root, "Старый").unwrap();
    let shaft = project
        .create_item_in(ItemKind::Part, &name("вал"), Path::new("parts"))
        .unwrap();
    let frame = project
        .create_item_in(ItemKind::Assembly, &name("рама"), Path::new("assemblies"))
        .unwrap();
    let axle = project
        .create_item_in(ItemKind::Part, &name("ось"), Path::new("parts"))
        .unwrap();
    let store = Store::open(project, &options()).unwrap();
    two_versions(&store, &shaft);
    store.commit(&frame, "рама").unwrap();
    fs::write(axle.dir.join("model.step"), "ось").unwrap();
    store.commit(&axle, "ось").unwrap();
    // Удаление так, как его записывала прежняя программа: без папки.
    git(root, &["rm", "-r", "-q", "parts/ось"]);
    let _ = fs::remove_dir_all(root.join("parts/ось"));
    git(
        root,
        &[
            "commit",
            "-q",
            "-m",
            "ось: деталь удалена\n\nNonius-Deleted: ось",
        ],
    );
    fs::write(root.join("project.yaml"), "schema: 1\nname: Старый\n").unwrap();
    git(root, &["commit", "-q", "-am", "старая схема"]);
    tmp
}

#[test]
fn old_project_is_read_only_until_upgraded() {
    let tmp = old_project();
    let root = tmp.path();
    let store = Store::open(Project::open(root).unwrap(), &options()).unwrap();
    assert!(store.project().is_old_layout());
    let shaft = store.item("вал").unwrap();
    assert_eq!(read(&store, &shaft, 1), "первая", "старое читается");
    assert!(matches!(
        store.commit(&shaft, "x"),
        Err(StoreError::Core(CoreError::OldLayout(_)))
    ));
    // Несохранённое переезжает вместе с деталью и остаётся несохранённым.
    fs::write(shaft.dir.join("model.step"), "правка").unwrap();

    let project = store.upgrade().unwrap();
    assert!(!project.is_old_layout());
    assert!(!root.join("parts").exists() && !root.join("assemblies").exists());
    let store = Store::open(project, &options()).unwrap();
    let shaft = store.item("вал").unwrap();
    assert_eq!(shaft.dir, root.join("вал"));
    assert_eq!(store.item("рама").unwrap().kind, ItemKind::Assembly);
    assert_eq!(read(&store, &shaft, 1), "первая");
    assert_eq!(read(&store, &shaft, 2), "вторая");
    assert_eq!(
        fs::read_to_string(shaft.dir.join("model.step")).unwrap(),
        "правка"
    );
    assert_eq!(store.changes(&shaft).unwrap().len(), 1);
    store.commit(&shaft, "после обновления").unwrap();
    assert_eq!(store.versions(&shaft).unwrap().len(), 3);

    // Удалённая до перехода возвращается в корень, с версией.
    let axle = store.recover(&name("ось")).unwrap();
    assert_eq!(axle.dir, root.join("ось"));
    assert!(!root.join("parts").exists());
    assert_eq!(read(&store, &axle, 1), "ось");
    assert!(store.changes(&axle).unwrap().is_empty());
}

#[test]
fn upgrade_refuses_when_root_has_the_same_name() {
    let tmp = old_project();
    fs::create_dir(tmp.path().join("вал")).unwrap();
    let store = Store::open(Project::open(tmp.path()).unwrap(), &options()).unwrap();
    assert!(matches!(
        store.upgrade(),
        Err(StoreError::Core(CoreError::AlreadyExists(_)))
    ));
    assert!(
        tmp.path().join("parts/вал/model.step").is_file(),
        "ничего не тронуто"
    );
    assert!(Project::open(tmp.path()).unwrap().is_old_layout());
}
