//! «Разобрать папку» в окне: изучение в фоне со строкой хода и
//! «Остановить», план, вопрос с кнопками, раскладка и итог; «Показать в 3D»
//! у вопроса и у детали с версиями. Проект — поддельный: что считает ядро,
//! проверяют тесты `nonius-app`.

use super::*;
use nonius_gui::source::{IntakeItem, IntakeOutcome, IntakePlan, IntakeQuestion, IntakeSide};

/// Вопрос поддельного плана.
const QUESTION: &str = "кронштейн|кронштейн v2";

/// Две модели кронштейна для «Показать в 3D».
fn sides() -> Vec<IntakeSide> {
    let side = |label: &str| IntakeSide {
        label: label.into(),
        models: vec![PathBuf::from(format!("папка/{label}.sldprt"))],
    };
    vec![side("кронштейн"), side("кронштейн v2")]
}

/// Что изменилось между ними — слова разбора.
const CHANGED: &str = "отверстие ⌀8 → ⌀10";

/// План поддельного проекта: пока на вопрос не ответили, версии кронштейна —
/// догадка; «нет» разводит их на две детали.
pub fn plan(answers: &[(String, bool)]) -> IntakePlan {
    let said = answers.iter().find(|(id, _)| id == QUESTION).map(|a| a.1);
    let part = |name: &str, versions: usize| IntakeItem {
        name: name.into(),
        versions,
        why: vec!["версии по именам: «кронштейн» → «кронштейн v2»".into()],
        docs: vec!["кронштейн.pdf".into()],
        unsure: said.is_none(),
        sides: if versions > 1 { sides() } else { Vec::new() },
        changed: vec![vec![CHANGED.into()]],
        ..IntakeItem::default()
    };
    let mut items = match said {
        Some(false) => vec![part("кронштейн", 1), part("кронштейн v2", 1)],
        _ => vec![part("кронштейн", 2)],
    };
    items.push(IntakeItem {
        name: "рама".into(),
        assembly: true,
        folder: "Стенд".into(),
        versions: 1,
        ..IntakeItem::default()
    });
    let questions = match said {
        Some(_) => Vec::new(),
        None => vec![IntakeQuestion {
            id: QUESTION.into(),
            text: "«кронштейн» и «кронштейн v2» — версии одной детали?".into(),
            facts: vec!["объём: 120 → 131 см³".into()],
            suggested: Some(true),
            sides: sides(),
            changed: vec![vec![CHANGED.into()]],
        }],
    };
    IntakePlan {
        source: PathBuf::from("папка"),
        items,
        questions,
        left: vec![("ТЗ.docx".into(), "непонятно, к какой детали".into())],
        other: 3,
    }
}

impl Fake {
    /// «Показать в 3D»: сцена с разницей, как у версий детали.
    pub fn look(
        &self,
        before: &[PathBuf],
        after: &[PathBuf],
        progress: &dyn Progress,
    ) -> anyhow::Result<Comparison> {
        progress.step("Беру STEP: кронштейн.sldprt…");
        // Тест держит расчёт, пока смотрит на окно.
        let _held = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        if *self.broken.lock().unwrap() {
            panic!("сбой сравнения");
        }
        let stopped = progress.cancelled();
        let line = format!("3D: {before:?} → {after:?}, остановлено: {stopped}");
        self.intake.lock().unwrap().push(line);
        if stopped {
            anyhow::bail!("сравнение остановлено");
        }
        // STEP первой модели старше её файла SolidWorks.
        let mut c = self.compare("", Pick::Version(1), Pick::Current, progress)?;
        c.step_behind = true;
        Ok(c)
    }

    pub fn study(&self, progress: &dyn Progress) -> anyhow::Result<IntakePlan> {
        progress.step("Смотрю форму деталей: 1 из 2");
        // Тест держит изучение, пока смотрит на строку хода.
        let _held = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        let stopped = progress.cancelled();
        let line = format!("изучено, остановлено: {stopped}");
        self.intake.lock().unwrap().push(line);
        Ok(plan(&[]))
    }

    pub fn apply(&self, answers: &[(String, bool)]) -> anyhow::Result<IntakeOutcome> {
        let line = format!("разложено: {answers:?}");
        self.intake.lock().unwrap().push(line);
        self.created.lock().unwrap().push(("плита".into(), false));
        Ok(IntakeOutcome {
            parts: plan(answers).items.len(),
            versions: 3,
            warnings: vec!["«рама»: STEP не получен".into()],
            failed: vec![("крышка".into(), "файл занят".into())],
            stopped: false,
        })
    }
}

fn window_for_folder(dir: &Path, project: Fake) -> Harness<'static, App> {
    std::env::set_var("NONIUS_CONFIG_DIR", dir);
    build(Options {
        project: Some(Box::new(project)),
        opener: Arc::new(|_| anyhow::bail!("не нужно")),
        creator: Arc::new(|_| anyhow::bail!("не нужно")),
        machine: Arc::new(Pc::default()),
        chooser: Arc::new(FolderIs(dir.join("свалка"))),
        part: None,
        picks: None,
        drafts: None,
        mover: Arc::new(|_, _| anyhow::bail!("не нужно")),
        discard: Arc::new(|d| Ok(std::fs::remove_dir_all(d)?)),
    })
}

fn open_intake(h: &mut Harness<'_, App>) {
    wait_for(h, "кронштейн");
    h.get_by_label("Файл").click();
    wait_for(h, "Разобрать папку…");
    h.get_by_label("Разобрать папку…").click();
}

#[test]
fn folder_is_sorted_with_a_question() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let log = project.intake.clone();
    let mut h = window_for_folder(dir.path(), project);
    open_intake(&mut h);

    // План: детали по папкам, «почему», пометка догадки, остатки.
    wait_for(&mut h, "деталь «кронштейн», версий: 2  (?)");
    wait_for(&mut h, "деталей — 1, сборок — 1, версий — 3");
    for text in [
        "В корень проекта:",
        "В папку «Стенд»:",
        "сборка «рама»",
        "версии по именам",
        "+ кронштейн.pdf",
        "ТЗ.docx — непонятно, к какой детали",
        "Ещё файлов: 3",
        "Вопрос (осталось 1)",
        "объём: 120 → 131 см³",
        "Похоже, что да.",
    ] {
        assert!(
            h.query_all_by_label_contains(text).next().is_some(),
            "на экране есть «{text}»"
        );
    }

    // «Потом» откладывает вопрос, «Спросить снова» возвращает.
    h.get_by_label("Потом").click();
    wait_for(&mut h, "Отложено вопросов: 1");
    assert!(h.query_by_label("Потом").is_none());
    h.get_by_label("Спросить снова").click();
    wait_for(&mut h, "Вопрос (осталось 1)");

    // «Нет» — план сразу другой: две детали, вопросов нет.
    h.get_by_label("Нет").click();
    wait_for(&mut h, "деталь «кронштейн v2»");
    assert!(h.query_by_label_contains("Вопрос (").is_none());
    assert!(h.query_by_label_contains("(?)").is_none());

    h.get_by_label("Разложить в проект").click();
    wait_for(&mut h, "Разложено: 3, версий сохранено: 3");
    wait_for(&mut h, "Не вышло: «крышка» — файл занят");
    wait_for(&mut h, "Предупреждение: «рама»: STEP не получен");
    // Дерево проекта перечитано: новая деталь на месте.
    wait_for(&mut h, "плита");
    h.get_by_label("Готово").click();
    h.run_steps(3);
    assert!(h.query_by_label("Разбор папки").is_none());
    let log = log.lock().unwrap();
    assert_eq!(log[0], "изучено, остановлено: false");
    assert!(log[1].contains("false"), "ответ «нет» дошёл: {}", log[1]);
}

#[test]
fn study_can_be_stopped() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let log = project.intake.clone();
    let gate = project.gate.clone();
    let held = gate.lock().unwrap();
    let mut h = window_for_folder(dir.path(), project);
    open_intake(&mut h);

    wait_for(&mut h, "Смотрю форму деталей: 1 из 2");
    h.get_by_label("Остановить").click();
    wait_for(&mut h, "Останавливаю");
    drop(held);
    // План — из того, что успели изучить.
    wait_for(&mut h, "Разложить в проект");
    assert_eq!(log.lock().unwrap()[0], "изучено, остановлено: true");

    // «Закрыть» убирает экран, ничего не раскладывая.
    h.get_by_label("Закрыть").click();
    h.run_steps(3);
    assert!(h.query_by_label("Разбор папки").is_none());
    assert_eq!(log.lock().unwrap().len(), 1);
}

/// Нажать n-ю кнопку «Показать в 3D»: 0 — у вопроса, 1 — у детали.
fn show_3d(h: &mut Harness<'_, App>, n: usize) {
    let buttons: Vec<_> = h.query_all_by_label("Показать в 3D").collect();
    assert_eq!(buttons.len(), 2, "кнопка у вопроса и у детали с версиями");
    buttons[n].click();
}

#[test]
fn question_shows_both_parts_in_3d() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let log = project.intake.clone();
    let gate = project.gate.clone();
    let broken = project.broken.clone();
    let mut h = window_for_folder(dir.path(), project);
    open_intake(&mut h);
    wait_for(&mut h, "Вопрос (осталось 1)");

    // Пока считается, окно живо: своя строка хода и «Остановить».
    let held = gate.lock().unwrap();
    show_3d(&mut h, 0);
    wait_for(&mut h, "Сравнение в 3D");
    wait_for(&mut h, "Беру STEP: кронштейн.sldprt…");
    assert!(h.query_by_label("«кронштейн» → «кронштейн v2»").is_some());
    assert!(h.query_by_label("Остановить").is_some());
    h.run_steps(5);
    drop(held);

    // Готово: цвета, режимы, слова разбора под картинкой.
    wait_for(
        &mut h,
        "Серое — общее, красное — убрано, зелёное — добавлено.",
    );
    for text in ["Было", "Изменения", "Стало", "Целиком", "Что изменилось:"]
    {
        assert!(h.query_by_label(text).is_some(), "на экране есть «{text}»");
    }
    assert!(h.query_by_label_contains(CHANGED).is_some());
    assert!(h.query_by_label_contains("может быть прежней").is_some());
    assert!(log.lock().unwrap()[1].contains("кронштейн v2.sldprt"));

    // «К плану» — снова вопрос, ответа никто не давал. Щелчок мышью: кнопку
    // не должен перекрывать 3D-вид.
    let back = h.get_by_label("← К плану").rect();
    super::click_with(&mut h, back, egui::Modifiers::NONE);
    wait_for(&mut h, "Вопрос (осталось 1)");

    // У детали своя кнопка; «Остановить» сразу возвращает к плану.
    let held = gate.lock().unwrap();
    show_3d(&mut h, 1);
    wait_for(&mut h, "Беру STEP");
    h.get_by_label("Остановить").click();
    wait_for(&mut h, "Вопрос (осталось 1)");
    drop(held);
    for _ in 0..300 {
        if log.lock().unwrap().len() == 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(log.lock().unwrap()[2].ends_with("остановлено: true"));
    h.run_steps(3);
    assert!(
        h.query_by_label("Сравнение в 3D").is_none(),
        "результат не нужен"
    );

    // Сбой расчёта не оставляет экран ждать вечно.
    *broken.lock().unwrap() = true;
    show_3d(&mut h, 0);
    wait_for(&mut h, "Внутренняя ошибка программы");
    wait_for(&mut h, "Вопрос (осталось 1)");
    assert!(h.query_by_label("Сравнение в 3D").is_none());
}
