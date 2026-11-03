//! Командная строка: папки проекта, карта, перенос детали, обновление
//! старого проекта.

use std::path::Path;
use std::process::Command;

fn nonius(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nonius"))
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Тестировщик")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Тестировщик")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env("NONIUS_NO_WINDOW", "1")
        .output()
        .unwrap();
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (out.status.success(), text)
}

fn ok(dir: &Path, args: &[&str]) -> String {
    let (ok, out) = nonius(dir, args);
    assert!(ok, "{args:?}: {out}");
    out
}

#[test]
fn folders_map_and_move() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    ok(d, &["init", "--name", "Мастерская"]);
    ok(d, &["folder", "new", "Стена/Рейка"]);
    ok(
        d,
        &[
            "part",
            "new",
            "RAIL-00",
            "--assembly",
            "--in",
            "Стена/Рейка",
        ],
    );
    ok(d, &["part", "new", "PV-01"]);
    std::fs::write(d.join("PV-01/model.step"), "форма").unwrap();
    ok(d, &["commit", "PV-01", "-m", "первая"]);

    let out = ok(d, &["part", "move", "PV-01", "Стена"]);
    assert!(out.contains("«Стена»"), "{out}");
    assert!(d.join("Стена/PV-01/model.step").is_file());

    let map = ok(d, &["--json", "map"]);
    let map: serde_json::Value = serde_json::from_str(&map).unwrap();
    let data = &map["data"];
    assert_eq!(data["old_layout"], false);
    assert!(data["folders"]
        .as_array()
        .unwrap()
        .contains(&"Стена/Рейка".into()));
    let path = |name: &str| {
        let items = data["items"].as_array().unwrap();
        let item = items.iter().find(|i| i["name"] == name).unwrap();
        item["path"].as_str().unwrap().to_string()
    };
    assert_eq!(path("PV-01"), "Стена/PV-01");
    assert_eq!(path("RAIL-00"), "Стена/Рейка/RAIL-00");

    let text = ok(d, &["map"]);
    assert!(text.contains("  Рейка/"), "{text}");
    assert!(ok(d, &["log", "PV-01"]).contains("первая"));

    // Назад в корень; внутрь детали — нельзя, с кодом ошибки.
    ok(d, &["part", "move", "PV-01", "."]);
    assert!(d.join("PV-01/model.step").is_file());
    let (fine, out) = nonius(
        d,
        &["--json", "part", "move", "PV-01", "Стена/Рейка/RAIL-00"],
    );
    assert!(!fine);
    assert!(out.contains("\"inside_item\""), "{out}");
}

#[test]
fn old_project_upgrades() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    ok(d, &["init", "--name", "Старый"]);
    // Так выглядел проект до папок.
    ok(d, &["folder", "new", "parts"]);
    ok(d, &["part", "new", "вал", "--in", "parts"]);
    std::fs::write(d.join("parts/вал/model.step"), "форма").unwrap();
    ok(d, &["commit", "вал", "-m", "первая"]);
    std::fs::write(d.join("project.yaml"), "schema: 1\nname: Старый\n").unwrap();

    let (fine, out) = nonius(d, &["--json", "commit", "вал", "-m", "x"]);
    assert!(!fine);
    assert!(out.contains("\"old_layout\""), "{out}");
    assert!(ok(d, &["info"]).contains("Формат:    старый"));

    let out = ok(d, &["upgrade"]);
    assert!(out.contains("Проект обновлён"), "{out}");
    assert!(d.join("вал/model.step").is_file());
    assert!(ok(d, &["log", "вал"]).contains("первая"));
    assert!(ok(d, &["upgrade"]).contains("обновлять нечего"));
}
