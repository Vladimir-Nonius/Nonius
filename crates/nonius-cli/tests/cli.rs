//! Сквозная проверка командной строки.

use std::process::Command;

fn nonius(dir: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nonius"))
        .current_dir(dir)
        .args(args)
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
}

#[test]
fn outside_project_gives_clear_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (ok, out) = nonius(tmp.path(), &["list"]);
    assert!(!ok);
    assert!(out.contains("нет проекта Nonius"), "{out}");
}
