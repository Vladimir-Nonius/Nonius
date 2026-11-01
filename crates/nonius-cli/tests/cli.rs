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

#[test]
fn agents_update_old_project_and_keep_people_text() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    assert!(nonius(d, &["init"]).0);
    let (ok, out) = nonius(d, &["agents"]);
    assert!(ok && out.contains("уже свежий"), "{out}");

    let file = d.join("AGENTS.md");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("nonius overview") && text.contains("model.*"),
        "{text}"
    );
    std::fs::write(&file, "# Свой файл\n\nНаш допуск 0,1 мм.\n").unwrap();
    let (ok, out) = nonius(d, &["overview"]);
    assert!(ok && out.contains("nonius agents"), "{out}");

    let (ok, out) = nonius(d, &["agents", "--ai", "Claude"]);
    assert!(ok && out.contains("обновлён"), "{out}");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("Наш допуск") && text.contains("nonius overview"),
        "{text}"
    );
    let (ok, out) = nonius(d, &["overview"]);
    assert!(ok && !out.contains("nonius agents"), "{out}");
}

/// Отправить серверу MCP сообщения и получить ответы.
fn mcp(
    dir: &std::path::Path,
    args: &[&str],
    messages: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_nonius"))
        .current_dir(dir)
        .arg("mcp")
        .args(args)
        .env("GIT_AUTHOR_NAME", "Тестировщик")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Тестировщик")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for m in messages {
        writeln!(stdin, "{m}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn call(id: u32, name: &str, arguments: serde_json::Value) -> serde_json::Value {
    let params = serde_json::json!({ "name": name, "arguments": arguments });
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": params })
}

fn tool_answer(reply: &serde_json::Value) -> serde_json::Value {
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    serde_json::from_str(text).unwrap()
}

#[test]
fn mcp_reads_by_default_and_signs_writes() {
    use serde_json::json;
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    assert!(nonius(d, &["init"]).0);
    assert!(nonius(d, &["part", "new", "вал"]).0);
    let hello = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-03-26", "clientInfo": { "name": "Claude Code" } },
    });
    let inited = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    let list = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });

    let out = mcp(
        d,
        &[],
        &[
            hello.clone(),
            inited.clone(),
            list.clone(),
            call(3, "overview", json!({})),
            call(4, "commit", json!({ "part": "вал", "message": "нельзя" })),
            call(5, "material", json!({ "part": "вал", "material": "Ст3" })),
            call(6, "log", json!({ "part": "нет" })),
            call(7, "diff", json!({ "part": "вал", "to": "v2" })),
            json!({ "jsonrpc": "2.0", "id": 8, "method": "нет" }),
        ],
    );
    assert_eq!(out.len(), 8, "на уведомление ответа нет: {out:?}");
    assert_eq!(out[0]["result"]["protocolVersion"], "2025-03-26");
    let tools = out[1]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(
        names.contains(&"overview") && !names.contains(&"commit"),
        "{names:?}"
    );
    let log = tools.iter().find(|t| t["name"] == "log").unwrap();
    assert_eq!(log["inputSchema"]["required"], json!(["part"]), "{log}");
    assert_eq!(tool_answer(&out[2])["data"]["new"], 1, "{}", out[2]);
    let code = |r: &serde_json::Value| tool_answer(r)["error"]["code"].clone();
    assert_eq!(code(&out[3]), "unknown_tool");
    assert_eq!(code(&out[4]), "read_only");
    assert_eq!(code(&out[5]), "unknown_item");
    assert_eq!(out[5]["result"]["isError"], true);
    assert_eq!(code(&out[6]), "usage");
    assert_eq!(out[7]["error"]["code"], -32601);

    let out = mcp(
        d,
        &["--write"],
        &[
            hello,
            list,
            call(3, "commit", json!({ "part": "вал", "message": "--json" })),
            call(4, "log", json!({ "part": "вал", "лишний": 1 })),
        ],
    );
    let tools = out[1]["result"]["tools"].to_string();
    assert!(tools.contains("commit"), "{tools}");
    let saved = tool_answer(&out[2]);
    assert_eq!(saved["data"]["ai"]["name"], "Claude Code", "{saved}");
    assert_eq!(saved["data"]["committer"], "Тестировщик", "{saved}");
    assert_eq!(saved["data"]["summary"], "--json", "{saved}");
    assert_eq!(tool_answer(&out[3])["error"]["code"], "usage");
}

fn json(dir: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let (_, out) = nonius(dir, args);
    serde_json::from_str(out.trim()).unwrap_or_else(|e| panic!("{e}: {out}"))
}

#[test]
fn json_answers_and_stable_error_codes() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    let code = |out: serde_json::Value| {
        assert_eq!(out["ok"], false, "{out}");
        out["error"]["code"].as_str().unwrap().to_string()
    };
    assert_eq!(code(json(d, &["list", "--json"])), "not_a_project");
    assert!(nonius(d, &["init"]).0);
    assert_eq!(code(json(d, &["--json", "нет-команды"])), "usage");
    assert_eq!(code(json(d, &["log", "нет", "--json"])), "unknown_item");
    assert_eq!(code(json(d, &["init", "--json"])), "no_json");

    let made = json(d, &["part", "new", "вал", "--json"]);
    assert_eq!(made["data"]["kind"], "part", "{made}");
    assert_eq!(code(json(d, &["diff", "вал", "--json"])), "no_versions");

    let saved = json(
        d,
        &["commit", "вал", "-m", "Первая", "--ai", "Claude", "--json"],
    );
    assert_eq!(saved["data"]["ref"], "вал@v1", "{saved}");
    assert_eq!(saved["data"]["ai"]["name"], "Claude", "{saved}");
    assert_eq!(saved["data"]["committer"], "Тестировщик", "{saved}");
    assert_eq!(
        code(json(d, &["commit", "вал", "-m", "ещё", "--json"])),
        "no_changes"
    );
    assert_eq!(
        code(json(d, &["restore", "вал", "v9", "--json"])),
        "unknown_version"
    );

    let top = json(d, &["overview", "--json"]);
    assert_eq!(top["data"]["saved"], 1, "{top}");
    assert_eq!(top["data"]["items"][0]["latest"]["version"], "v1", "{top}");

    let docs = d.join("parts").join("вал").join("docs");
    std::fs::write(docs.join("чертёж.txt"), "сталь").unwrap();
    let status = json(d, &["status", "--json"]);
    assert_eq!(status["data"]["items"][0]["state"], "changed", "{status}");
    let diff = json(d, &["diff", "вал", "--json"]);
    assert_eq!(diff["data"]["files"][0]["change"], "added", "{diff}");
    assert_eq!(diff["data"]["to"], serde_json::Value::Null, "{diff}");
    let lines = json(d, &["files", "вал", "--text", "docs/чертёж.txt", "--json"]);
    assert_eq!(lines["data"]["lines"][0]["kind"], "added", "{lines}");
    let material = json(d, &["material", "вал", "Ст3", "--json"]);
    assert_eq!(material["data"]["mass"]["missing"], "volume", "{material}");
}

#[test]
fn describe_lists_commands_and_codes() {
    let tmp = tempfile::tempdir().unwrap();
    let out = json(tmp.path(), &["describe"]);
    let names: Vec<&str> = out["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"overview") && names.contains(&"part new"),
        "{names:?}"
    );
    let commit = out["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "commit")
        .unwrap();
    assert_eq!(commit["json"], true);
    assert!(commit["args"].to_string().contains("--message"), "{commit}");
    assert!(out["options"].to_string().contains("NONIUS_AI"), "{out}");
    assert!(
        out["errors"].to_string().contains("unknown_version"),
        "{out}"
    );
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
