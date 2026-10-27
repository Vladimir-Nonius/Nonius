//! `nonius view`: понятные ошибки до запуска окна.

use std::process::Command;

fn nonius(dir: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nonius"))
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Тестировщик")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Тестировщик")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (out.status.success(), text)
}

/// STEP только с заголовком, без геометрии.
const STEP: &str = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\nFILE_NAME('кронштейн','2026-10-26',('Владимир'),(''),'SwSTEP 2.0','SolidWorks 2020','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n";

#[test]
fn view_explains_what_is_wrong() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    assert!(nonius(d, &["init"]).0);

    let (ok, out) = nonius(d, &["view", "кронштейн", "--no-browser"]);
    assert!(!ok);
    assert!(out.contains("нет детали «кронштейн»"), "{out}");

    std::fs::write(d.join("кронштейн.step"), STEP).unwrap();
    assert!(nonius(d, &["import", "кронштейн.step"]).0);

    // Без OpenCascade — объяснение; с ним — ошибка чтения пустого STEP.
    // Окно не запускается, программа не зависает.
    let (ok, out) = nonius(d, &["view", "кронштейн", "--no-browser"]);
    assert!(!ok, "{out}");
    let geometry = nonius(d, &["modules"]).1.contains("scene-occt");
    if geometry {
        assert!(out.contains("scene-occt"), "{out}");
    } else {
        assert!(
            out.contains("доступен в сборке Nonius с OpenCascade"),
            "{out}"
        );
    }
    assert!(!out.contains("Просмотр: http"), "{out}");
}
