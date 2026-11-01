//! Сквозная проверка командной строки.

use std::process::Command;

fn nonius(dir: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nonius"))
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Тестировщик")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Тестировщик")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        // Окно в проверках не открываем.
        .env("NONIUS_NO_WINDOW", "1")
        .output()
        .unwrap();
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (out.status.success(), text)
}

#[test]
fn init_add_and_list() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();

    let (ok, out) = nonius(d, &["init", "--name", "Станок"]);
    assert!(ok, "{out}");
    assert!(out.contains("Станок"));

    assert!(nonius(d, &["part", "new", "кронштейн"]).0);
    assert!(nonius(d, &["part", "new", "рама", "--assembly"]).0);

    let (ok, out) = nonius(d, &["list"]);
    assert!(ok, "{out}");
    assert!(out.contains("деталь") && out.contains("кронштейн"), "{out}");
    assert!(out.contains("сборка") && out.contains("рама"), "{out}");

    let (ok, out) = nonius(d, &["part", "new", "вал@v2"]);
    assert!(!ok);
    assert!(out.contains("запрещённый символ"), "{out}");

    let (ok, out) = nonius(&d.join("parts"), &["info"]);
    assert!(ok, "{out}");
    assert!(out.contains("Деталей:   1"), "{out}");

    let (ok, out) = nonius(d, &["modules"]);
    assert!(ok && out.contains("meta-compare"), "{out}");
    assert!(out.contains("step-header"), "{out}");
}

#[test]
fn no_command_without_window_explains_how_to_run() {
    let tmp = tempfile::tempdir().unwrap();
    let (ok, out) = nonius(tmp.path(), &[]);
    assert!(ok, "{out}");
    assert!(out.contains("PowerShell"), "{out}");
    assert!(out.contains(".\\nonius.exe --help"), "{out}");
    assert!(out.contains("import"), "список команд: {out}");
}

#[test]
fn outside_project_gives_clear_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (ok, out) = nonius(tmp.path(), &["list"]);
    assert!(!ok);
    assert!(out.contains("нет проекта Nonius"), "{out}");
}

#[test]
fn overview_and_ai_signature() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    assert!(nonius(d, &["init", "--name", "Станок"]).0);
    let (ok, out) = nonius(d, &["overview"]);
    assert!(ok && out.contains("Пока пусто"), "{out}");

    assert!(nonius(d, &["part", "new", "вал"]).0);
    assert!(nonius(d, &["part", "new", "рама"]).0);
    let saved = nonius(
        d,
        &[
            "commit",
            "вал",
            "-m",
            "Первая",
            "--ai",
            "Claude",
            "--ai-model",
            "claude-opus-5-5",
        ],
    );
    assert!(saved.0, "{}", saved.1);

    let (ok, out) = nonius(d, &["overview"]);
    assert!(ok, "{out}");
    assert!(
        out.starts_with("Проект «Станок»: деталей 2, сборок 0. Сохранено 1, изменено 0, новых 1."),
        "{out}"
    );
    assert!(out.contains("вал | деталь | v1 из 1 |"), "{out}");
    assert!(
        out.contains("Claude (ИИ, claude-opus-5-5, запустил Тестировщик) | Первая | сохранена"),
        "{out}"
    );
    assert!(out.contains("рама | деталь | новая"), "{out}");

    let (ok, out) = nonius(d, &["overview", "--changed"]);
    assert!(
        ok && out.contains("рама") && !out.contains("вал |"),
        "{out}"
    );

    let (ok, out) = nonius(d, &["log", "вал"]);
    assert!(ok && out.contains("Claude (ИИ"), "{out}");
}

#[test]
fn diff_between_versions_lists_files() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    assert!(nonius(d, &["init"]).0);
    assert!(nonius(d, &["part", "new", "вал"]).0);
    assert!(nonius(d, &["commit", "вал", "-m", "Первая"]).0);
    let docs = d.join("parts").join("вал").join("docs");
    std::fs::write(docs.join("чертёж.pdf"), "pdf").unwrap();
    assert!(nonius(d, &["commit", "вал", "-m", "Чертёж"]).0);

    let (ok, out) = nonius(d, &["diff", "вал", "v1", "v2"]);
    assert!(ok, "{out}");
    assert!(out.contains("+ файл docs/чертёж.pdf"), "{out}");
    assert!(!out.contains("изменений нет"), "{out}");
}

const STEP: &str = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\nFILE_NAME('кронштейн','2026-10-26',('Владимир'),(''),'SwSTEP 2.0','SolidWorks 2020','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n";

#[test]
fn import_commit_log_diff_restore_export() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    let (ok, out) = nonius(d, &["init", "--name", "Станок"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("История версий включена") || out.contains("Git LFS не найден"),
        "{out}"
    );

    let step = d.join("кронштейн.step");
    std::fs::write(&step, STEP).unwrap();
    // В этом STEP только заголовок, без геометрии: импорт не должен ломаться
    // и с OpenCascade — сведения берутся из заголовка.
    let (ok, out) = nonius(d, &["import", "кронштейн.step"]);
    assert!(ok, "{out}");
    assert!(out.contains("Источник: SolidWorks 2020"), "{out}");

    let (ok, out) = nonius(d, &["status"]);
    assert!(
        ok && out.contains("ещё нет версий") && out.contains("model.step"),
        "{out}"
    );

    let (ok, out) = nonius(d, &["commit", "кронштейн", "-m", "Первая версия"]);
    assert!(ok, "{out}");
    assert!(out.contains("v1"), "{out}");

    let (ok, out) = nonius(d, &["commit", "кронштейн", "-m", "ещё"]);
    assert!(!ok && out.contains("нет изменений"), "{out}");

    // меняем материал вручную в meta.yaml и сравниваем с v1
    let meta = d.join("parts").join("кронштейн").join("meta.yaml");
    let text = std::fs::read_to_string(&meta).unwrap() + "material: Ст3\n";
    std::fs::write(&meta, text).unwrap();
    let (ok, out) = nonius(d, &["diff", "кронштейн"]);
    assert!(ok, "{out}");
    assert!(out.contains("+ Материал: добавлено (Ст3)"), "{out}");
    assert!(out.contains("~ файл meta.yaml"), "{out}");

    assert!(nonius(d, &["commit", "кронштейн", "-m", "Материал Ст3"]).0);
    let (ok, out) = nonius(d, &["log", "кронштейн"]);
    assert!(ok, "{out}");
    let v2 = out.find("v2").expect("есть v2");
    let v1 = out.find("v1").expect("есть v1");
    assert!(v2 < v1, "новые версии первыми: {out}");
    assert!(
        out.contains("Тестировщик") && out.contains("Материал Ст3"),
        "{out}"
    );

    let (ok, out) = nonius(d, &["diff", "кронштейн", "v1", "v2"]);
    assert!(ok && out.contains("Материал"), "{out}");

    let (ok, out) = nonius(d, &["restore", "кронштейн", "1", "-m", "Ст3 не нужна"]);
    assert!(ok, "{out}");
    assert!(out.contains("v3"), "{out}");
    assert!(!std::fs::read_to_string(&meta).unwrap().contains("Ст3"));

    let (ok, out) = nonius(d, &["export", "кронштейн", "v2", "выгрузка"]);
    assert!(ok, "{out}");
    let exported = std::fs::read_to_string(d.join("выгрузка").join("meta.yaml")).unwrap();
    assert!(exported.contains("Ст3"));
    assert_eq!(
        std::fs::read_to_string(d.join("выгрузка").join("model.step")).unwrap(),
        STEP
    );

    let (ok, out) = nonius(d, &["status", "кронштейн"]);
    assert!(ok && out.contains("изменений нет"), "{out}");
    let (ok, out) = nonius(d, &["log", "нет-такой"]);
    assert!(!ok && out.contains("нет детали"), "{out}");
}

#[test]
fn material_sets_mass_only_when_known() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    assert!(nonius(d, &["init", "--name", "Станок"]).0);
    assert!(nonius(d, &["part", "new", "кронштейн"]).0);

    let (ok, out) = nonius(d, &["material", "кронштейн"]);
    assert!(ok, "{out}");
    assert!(out.contains("Материал: не задан"), "{out}");

    let (ok, out) = nonius(d, &["material", "кронштейн", "09Г2С"]);
    assert!(ok, "{out}");
    assert!(out.contains("плотность 7850 кг/м³"), "{out}");
    let meta = std::fs::read_to_string(d.join("parts/кронштейн/meta.yaml")).unwrap();
    assert!(meta.contains("material: 09Г2С"), "{meta}");

    let (ok, out) = nonius(d, &["material", "кронштейн", "Сплав-Х"]);
    assert!(ok, "{out}");
    assert!(out.contains("Материал: Сплав-Х"), "{out}");

    let (ok, out) = nonius(d, &["material", "кронштейн", "Сплав-Х", "--density", "0"]);
    assert!(!ok && out.contains("больше нуля"), "{out}");

    let (ok, out) = nonius(d, &["material", "кронштейн", "--remove"]);
    assert!(ok && out.contains("Материал: не задан"), "{out}");

    let (ok, out) = nonius(d, &["materials"]);
    assert!(ok && out.contains("АМг6") && out.contains("2640"), "{out}");
}

#[test]
fn density_needs_material() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    assert!(nonius(d, &["init"]).0);
    assert!(nonius(d, &["part", "new", "вал"]).0);
    let (ok, _) = nonius(d, &["material", "вал", "--density", "7800"]);
    assert!(!ok, "плотность без материала не принимается");
}

#[test]
fn files_between_versions() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    assert!(nonius(d, &["init", "--name", "Станок"]).0);
    assert!(nonius(d, &["part", "new", "кронштейн"]).0);
    let docs = d.join("parts").join("кронштейн").join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("заметка.md"), "сверлить Ø6\n").unwrap();
    assert!(nonius(d, &["commit", "кронштейн", "-m", "Первая"]).0);
    std::fs::write(docs.join("заметка.md"), "сверлить Ø8\n").unwrap();
    std::fs::write(docs.join("чертёж.pdf"), b"%PDF").unwrap();

    let (ok, out) = nonius(d, &["files", "кронштейн"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("изменён") && out.contains("docs/заметка.md"),
        "{out}"
    );
    assert!(
        out.contains("новый") && out.contains("docs/чертёж.pdf"),
        "{out}"
    );

    let (ok, out) = nonius(d, &["files", "кронштейн", "--text", "docs/заметка.md"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("- сверлить Ø6") && out.contains("+ сверлить Ø8"),
        "{out}"
    );

    // Файла нет — программа не запускается, а говорит словами.
    let (ok, out) = nonius(d, &["open", "кронштейн", "нет.pdf"]);
    assert!(!ok);
    assert!(out.contains("нет файла"), "{out}");
}
