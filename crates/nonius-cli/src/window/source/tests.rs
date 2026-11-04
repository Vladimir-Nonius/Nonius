//! Проверки моста к окну на настоящем проекте во временной папке.

use std::sync::Mutex;

use nonius_scene::Scene;

use super::*;
use crate::registry;

/// Ход работы для проверки: запоминает шаги, может «нажать Отменить».
#[derive(Default)]
struct Said {
    steps: Mutex<Vec<String>>,
    models: Mutex<usize>,
    cancel: bool,
}

impl Progress for Said {
    fn step(&self, what: &str) {
        self.steps.lock().unwrap().push(what.to_string());
    }
    fn model(&self, _scene: Scene) {
        *self.models.lock().unwrap() += 1;
    }
    fn cancelled(&self) -> bool {
        self.cancel
    }
}

const STEP: &str = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\nFILE_NAME('кронштейн','2026-10-26',('Владимир'),(''),'SwSTEP 2.0','SolidWorks 2020','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n";

#[test]
fn store_feeds_the_window() {
    for (k, v) in [
        ("GIT_AUTHOR_NAME", "Тестировщик"),
        ("GIT_AUTHOR_EMAIL", "test@example.com"),
        ("GIT_COMMITTER_NAME", "Тестировщик"),
        ("GIT_COMMITTER_EMAIL", "test@example.com"),
    ] {
        std::env::set_var(k, v);
    }
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    nonius_app::create_project(d, Some("Станок")).unwrap();
    let step = d.join("кронштейн.step");
    std::fs::write(&step, STEP).unwrap();
    let app = App::open(d, registry()).unwrap();
    app.import(&step, None).unwrap();
    app.save_version("кронштейн", "Первая").unwrap();
    let meta = d.join("кронштейн").join("meta.yaml");
    let text = std::fs::read_to_string(&meta).unwrap() + "material: Ст3\n";
    std::fs::write(&meta, text).unwrap();
    app.save_version("кронштейн", "Материал").unwrap();

    // Из подпапки проекта — тоже находится.
    let src = AppSource::open(&d.join("notes")).unwrap();
    assert_eq!(src.title(), "Станок");
    let parts = src.parts().unwrap();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].name, "кронштейн");
    assert_eq!(parts[0].latest.as_deref(), Some("v2"));
    assert!(!parts[0].dirty);

    let versions = src.versions("кронштейн").unwrap();
    let numbers: Vec<_> = versions.iter().map(|v| v.number).collect();
    assert_eq!(numbers, [2, 1], "новые первыми");
    assert_eq!(versions[0].summary, "Материал");
    assert_eq!(versions[0].author, "Тестировщик");

    let said = Said::default();
    let c = src
        .compare("кронштейн", Pick::Version(1), Pick::Version(2), &said)
        .unwrap();
    let lines: Vec<String> = c
        .reports
        .iter()
        .flat_map(|r| &r.changes)
        .map(|ch| ch.to_string())
        .collect();
    assert!(lines.iter().any(|l| l.contains("Материал")), "{lines:?}");
    // В STEP только заголовок — показывать в 3D нечего, а числа есть.
    assert!(c.scene.is_none());
    if let Some(p) = &c.scene_problem {
        assert!(p.contains("нет геометрии"), "{p}");
    }
    assert!(
        !said.steps.lock().unwrap().is_empty(),
        "окно видит ход работы"
    );
    let quiet = Said::default();
    assert!(src
        .compare("нет такой", Pick::Version(1), Pick::Current, &quiet)
        .is_err());
    assert!(src
        .compare("кронштейн", Pick::Current, Pick::Version(1), &quiet)
        .is_err());
    // «Отменить» — ни сравнения, ни подмены одними числами.
    let cancelled = Said {
        cancel: true,
        ..Said::default()
    };
    let e = src
        .compare("кронштейн", Pick::Version(2), Pick::Current, &cancelled)
        .unwrap_err();
    assert!(format!("{e:#}").contains("отменен"), "{e:#}");
}

#[test]
fn project_and_model_from_the_window() {
    for (k, v) in [
        ("GIT_AUTHOR_NAME", "Тестировщик"),
        ("GIT_AUTHOR_EMAIL", "test@example.com"),
        ("GIT_COMMITTER_NAME", "Тестировщик"),
        ("GIT_COMMITTER_EMAIL", "test@example.com"),
    ] {
        std::env::set_var(k, v);
    }
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path().join("Станок");
    std::fs::create_dir(&d).unwrap();
    let src = AppSource::create(&d).unwrap();
    assert_eq!(src.title(), "Станок", "название — имя папки");
    assert!(
        AppSource::create(&d).is_err(),
        "второй раз проект не создаётся"
    );

    src.new_part("корпус", true, "").unwrap();
    assert!(src.new_part("корпус", false, "").is_err(), "имя занято");
    let step = tmp.path().join("пластина.step");
    std::fs::write(&step, STEP).unwrap();
    let added = src.add_model(&step, "пластина", "Первая").unwrap();
    assert_eq!((added.part.as_str(), added.version), ("пластина", Some(1)));
    let added = src.add_model(&step, "корпус", "Модель").unwrap();
    assert_eq!((added.part.as_str(), added.version), ("корпус", Some(1)));
    let parts = src.parts().unwrap();
    assert_eq!(parts.len(), 2);
    assert!(parts.iter().all(|p| p.model.is_some()), "{parts:?}");
    assert!(
        parts.iter().all(|p| !p.dirty),
        "версии сохранены: {parts:?}"
    );
    assert!(parts.iter().any(|p| p.name == "корпус" && p.assembly));

    // Тот же файл ещё раз: регистр не важен, новой версии нет.
    let added = src.add_model(&step, "ПЛАСТИНА", "Снова").unwrap();
    assert_eq!((added.part.as_str(), added.version), ("пластина", None));
    assert_eq!(src.parts().unwrap().len(), 2);

    // Деталь без единой версии видна как есть: сравнивать не требуется.
    let said = Said::default();
    src.new_part("пустая", false, "").unwrap();
    let shown = src
        .compare("пустая", Pick::Current, Pick::Current, &said)
        .unwrap();
    assert!(shown.reports.is_empty());
}

/// Весь путь на настоящей геометрии: без OpenCascade проверять нечего.
#[test]
fn model_first_then_colors() {
    if !nonius_occt::available() {
        return;
    }
    for (k, v) in [
        ("GIT_AUTHOR_NAME", "Тестировщик"),
        ("GIT_AUTHOR_EMAIL", "test@example.com"),
        ("GIT_COMMITTER_NAME", "Тестировщик"),
        ("GIT_COMMITTER_EMAIL", "test@example.com"),
    ] {
        std::env::set_var(k, v);
    }
    let smoke = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/smoke");
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path().join("Станок");
    std::fs::create_dir(&d).unwrap();
    let src = AppSource::create(&d).unwrap();
    let roles = |c: &Comparison| -> Vec<nonius_scene::Role> {
        let scene = c.scene.as_ref().expect("сцена есть");
        scene.regions.iter().map(|r| r.role).collect()
    };
    use nonius_scene::Role;

    // Модель видна и до первой версии: сравнивать не требуется.
    src.new_part("плита", false, "").unwrap();
    let v1 = smoke.join("plate-v1.step");
    let added = src.add_model(&v1, "плита", "Первая").unwrap();
    assert_eq!(added.version, Some(1), "{:?}", added.warnings);
    let said = Said::default();
    let one = src
        .compare("плита", Pick::Version(1), Pick::Version(1), &said)
        .unwrap();
    assert_eq!(roles(&one), [Role::After]);
    assert!(one.reports.is_empty());
    assert_eq!(*said.models.lock().unwrap(), 0, "цвета не считаются");

    let v2 = smoke.join("plate-v2.step");
    assert_eq!(
        src.add_model(&v2, "плита", "Правка").unwrap().version,
        Some(2)
    );
    let said = Said::default();
    let two = src
        .compare("плита", Pick::Version(1), Pick::Version(2), &said)
        .unwrap();
    assert_eq!(*said.models.lock().unwrap(), 1, "сначала деталь");
    assert!(roles(&two).contains(&Role::Before), "{:?}", roles(&two));
    assert!(roles(&two).iter().any(|r| r.is_change()), "цвета есть");
    assert!(!two.step_behind);
    assert!(two.reports.iter().any(|r| !r.changes.is_empty()));

    // Заметка не сбрасывает расчёт: результат тот же и из кэша.
    let docs = d.join("плита").join("docs");
    std::fs::write(docs.join("заметка.md"), "сверлить").unwrap();
    let said = Said::default();
    let again = src
        .compare("плита", Pick::Version(2), Pick::Current, &said)
        .unwrap();
    assert_eq!(roles(&again), [Role::After], "форма та же");

    // SLDPRT поверх STEP: форма в окне старая, и окно это говорит.
    let sw = tmp.path().join("плита.sldprt");
    std::fs::write(&sw, "файл SolidWorks").unwrap();
    let added = src.add_model(&sw, "плита", "Из SolidWorks").unwrap();
    assert!(added.warnings.iter().any(|w| w.contains("STEP")));
    let behind = src
        .compare(
            "плита",
            Pick::Version(2),
            Pick::Version(3),
            &Said::default(),
        )
        .unwrap();
    assert!(behind.step_behind);
}

#[test]
fn broken_model_changes_nothing() {
    for (k, v) in [
        ("GIT_AUTHOR_NAME", "Тестировщик"),
        ("GIT_AUTHOR_EMAIL", "test@example.com"),
        ("GIT_COMMITTER_NAME", "Тестировщик"),
        ("GIT_COMMITTER_EMAIL", "test@example.com"),
    ] {
        std::env::set_var(k, v);
    }
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path().join("Станок");
    std::fs::create_dir(&d).unwrap();
    let src = AppSource::create(&d).unwrap();
    let good = tmp.path().join("пластина.step");
    std::fs::write(&good, STEP).unwrap();
    src.add_model(&good, "пластина", "Первая").unwrap();

    let bad = tmp.path().join("мусор.step");
    std::fs::write(&bad, "мусор").unwrap();
    assert!(src.add_model(&bad, "новая", "Нет").is_err());
    assert!(src.add_model(&bad, "пластина", "Нет").is_err());
    let parts = src.parts().unwrap();
    assert_eq!(parts.len(), 1, "деталь-призрак: {parts:?}");
    assert!(!parts[0].dirty, "рабочая модель цела");
    let model = d.join("пластина").join("model.step");
    assert_eq!(std::fs::read_to_string(model).unwrap(), STEP);
}

#[test]
fn no_project_inside_a_project() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    nonius_app::create_project(d, Some("Станок")).unwrap();
    let inner = d.join("notes");
    let e = nonius_app::create_project(&inner, None).unwrap_err();
    assert_eq!(e.code(), "inside_project");
    assert!(e.to_string().contains("Станок"), "{e}");
}

#[test]
fn material_and_saving_from_the_window() {
    for (k, v) in [
        ("GIT_AUTHOR_NAME", "Тестировщик"),
        ("GIT_AUTHOR_EMAIL", "test@example.com"),
        ("GIT_COMMITTER_NAME", "Тестировщик"),
        ("GIT_COMMITTER_EMAIL", "test@example.com"),
    ] {
        std::env::set_var(k, v);
    }
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    nonius_app::create_project(d, Some("Станок")).unwrap();
    let step = d.join("пластина.step");
    std::fs::write(&step, STEP).unwrap();
    App::open(d, registry())
        .unwrap()
        .import(&step, None)
        .unwrap();
    let src = AppSource::open(d).unwrap();

    // Новая деталь без версий — есть что сохранить.
    assert!(src.parts().unwrap()[0].dirty);
    assert!(!src.changed_files("пластина").unwrap().is_empty());
    assert!(src.save_version("пластина", "  ").is_err(), "без описания");
    assert_eq!(src.save_version("пластина", "Первая").unwrap(), 1);
    assert!(!src.parts().unwrap()[0].dirty);

    let none = src.mass("пластина", Pick::Version(1)).unwrap();
    assert_eq!(none.material, None);
    assert_eq!(none.mass_kg, None);
    assert!(none.no_mass.unwrap().contains("материал не задан"));

    // Из справочника плотность берётся сама.
    let st3 = src.set_material("пластина", Some("Ст3"), None).unwrap();
    assert_eq!(st3.material.as_deref(), Some("Ст3"));
    assert_eq!(st3.density_kg_m3, Some(7850.0));
    assert!(src.materials().iter().any(|m| m.name == "Ст3"));
    let parts = src.parts().unwrap();
    assert!(parts[0].dirty, "материал меняет деталь");
    assert!(src
        .changed_files("пластина")
        .unwrap()
        .iter()
        .any(|f| f.contains("meta.yaml")));

    // Свой материал — с плотностью вручную.
    let own = src
        .set_material("пластина", Some("Сплав заказчика"), Some(2850.0))
        .unwrap();
    assert_eq!(own.density_kg_m3, Some(2850.0));
    assert_eq!(src.save_version("пластина", "Материал").unwrap(), 2);
    let v2 = src.mass("пластина", Pick::Version(2)).unwrap();
    assert_eq!(v2.material.as_deref(), Some("Сплав заказчика"));

    // Убрать материал — массы снова нет.
    let cleared = src.set_material("пластина", None, None).unwrap();
    assert_eq!(cleared.material, None);
    assert!(cleared.no_mass.is_some());
}

#[test]
fn folders_from_the_window() {
    for (k, v) in [
        ("GIT_AUTHOR_NAME", "Тестировщик"),
        ("GIT_AUTHOR_EMAIL", "test@example.com"),
        ("GIT_COMMITTER_NAME", "Тестировщик"),
        ("GIT_COMMITTER_EMAIL", "test@example.com"),
    ] {
        std::env::set_var(k, v);
    }
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path().join("Мастерская");
    std::fs::create_dir(&d).unwrap();
    let src = AppSource::create(&d).unwrap();

    src.new_folder("Стена/Рейка").unwrap();
    // notes/ — папка для заметок, она тоже видна в дереве.
    assert_eq!(src.folders().unwrap(), ["notes", "Стена", "Стена/Рейка"]);
    src.new_part("RAIL-01", false, "Стена/Рейка").unwrap();
    assert!(d.join("Стена/Рейка/RAIL-01/meta.yaml").is_file());
    let listing = src.listing().unwrap();
    assert_eq!(listing.parts[0].folder, "Стена/Рейка");

    // Деталь без версий переносится просто как папка.
    src.move_part("RAIL-01", "Стена/Рейка/..").unwrap_err();
    src.move_part("RAIL-01", "").unwrap();
    assert_eq!(src.parts().unwrap()[0].folder, "");
    let step = tmp.path().join("RAIL-01.step");
    std::fs::write(&step, STEP).unwrap();
    src.add_model(&step, "RAIL-01", "Первая").unwrap();
    src.move_part("RAIL-01", "Стена").unwrap();
    assert_eq!(src.parts().unwrap()[0].folder, "Стена");
    src.move_part("RAIL-01", "").unwrap();
    assert_eq!(src.parts().unwrap()[0].folder, "");
    assert_eq!(src.versions("RAIL-01").unwrap().len(), 1, "версия осталась");

    let e = src.new_folder("RAIL-01/внутри").unwrap_err();
    assert!(format!("{e:#}").contains("внутрь детали"), "{e:#}");
}
