//! Разбор папки через мост к окну: изучение, ответ на вопрос, раскладка —
//! на настоящем проекте во временной папке.

use nonius_gui::source::Progress;

use super::super::AppSource;

struct Quiet;

impl Progress for Quiet {
    fn step(&self, _what: &str) {}
}

const STEP: &str = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\nFILE_NAME('вал','2026-10-26',('Владимир'),(''),'SwSTEP 2.0','SolidWorks 2020','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n";

#[test]
fn folder_goes_into_the_project_from_the_window() {
    for (k, v) in [
        ("GIT_AUTHOR_NAME", "Тестировщик"),
        ("GIT_AUTHOR_EMAIL", "test@example.com"),
        ("GIT_COMMITTER_NAME", "Тестировщик"),
        ("GIT_COMMITTER_EMAIL", "test@example.com"),
    ] {
        std::env::set_var(k, v);
    }
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("проект");
    let pile = tmp.path().join("свалка");
    std::fs::create_dir_all(pile.join("Рама")).unwrap();
    nonius_app::create_project(&project, Some("Станок")).unwrap();
    std::fs::write(pile.join("вал.step"), STEP).unwrap();
    // Вторая версия отличается содержимым: иначе это побайтная копия.
    std::fs::write(
        pile.join("вал v2.step"),
        STEP.replace("2026-10-26", "2026-11-02"),
    )
    .unwrap();
    std::fs::write(pile.join("Рама").join("плита.step"), STEP).unwrap();
    std::fs::write(pile.join("заметки.txt"), "не модель").unwrap();

    let src = AppSource::open(&project).unwrap();
    // Пока папку не изучили, ни плана, ни раскладки.
    assert!(src.intake_plan(&[]).is_err());
    assert!(src.intake_apply(&[], &Quiet).is_err());

    let plan = src.intake_study(&pile, &Quiet).unwrap();
    let names: Vec<_> = plan.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["вал", "плита"], "версии вала — одна деталь");
    assert_eq!(plan.items[0].versions, 2);
    assert_eq!(plan.items[1].folder, "Рама");
    assert_eq!(plan.other, 1, "заметки не тронуты");
    // Пересчёт без ответов — тот же план.
    assert_eq!(src.intake_plan(&[]).unwrap(), plan);

    // Ответ «нет» на вопрос о версиях разводит их на две детали.
    let mut answers = Vec::new();
    if let Some(q) = plan.questions.first() {
        answers.push((q.id.clone(), false));
        let split = src.intake_plan(&answers).unwrap();
        assert_eq!(split.items.len(), 3, "{split:#?}");
        assert!(split.questions.iter().all(|other| other.id != q.id));
    }

    let done = src.intake_apply(&answers, &Quiet).unwrap();
    assert!(done.failed.is_empty(), "{:?}", done.failed);
    assert_eq!(done.parts, 2 + answers.len());
    assert_eq!(done.versions, 3);
    assert!(!done.stopped);
    let parts = src.parts().unwrap();
    assert_eq!(parts.len(), done.parts);
    assert!(parts
        .iter()
        .any(|p| p.name == "плита" && p.folder == "Рама"));
    assert!(parts.iter().all(|p| !p.dirty), "версии сохранены");
    // Разложенное второй раз не раскладывается: папку изучают заново.
    assert!(src.intake_apply(&answers, &Quiet).is_err());
    // Исходная папка цела.
    assert!(pile.join("вал v2.step").is_file());
}
