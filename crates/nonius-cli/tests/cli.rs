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
