//! `nonius describe`: все команды, параметры и коды ошибок в JSON. Список
//! берётся из самого разбора команд, поэтому не расходится с программой.

use clap::CommandFactory;
use nonius_app::ERROR_CODES;
use serde_json::{json, Value};

use crate::args::Cli;
use crate::commands::json::COMMANDS;

/// Описание для программ и ИИ.
pub fn describe() -> Value {
    let root = Cli::command();
    let mut commands = Vec::new();
    for c in root.get_subcommands() {
        collect(c, c.get_name(), &mut commands);
    }
    let errors: Vec<Value> = ERROR_CODES
        .iter()
        .map(|(code, meaning)| json!({ "code": code, "meaning": meaning }))
        .collect();
    json!({
        "name": root.get_name(),
        "version": env!("CARGO_PKG_VERSION"),
        "options": args(&root),
        "commands": commands,
        "errors": errors,
        "exit_codes": { "0": "успех", "1": "ошибка действия", "2": "неверная команда" },
        "json": {
            "success": { "ok": true, "data": "ответ команды" },
            "failure": { "ok": false, "error": { "code": "постоянный код", "message": "для человека" } },
        },
        "ai": "ИИ подписывает версии: --ai и --ai-model или NONIUS_AI и NONIUS_AI_MODEL",
    })
}

fn collect(command: &clap::Command, path: &str, out: &mut Vec<Value>) {
    if command.has_subcommands() {
        for c in command.get_subcommands() {
            collect(c, &format!("{path} {}", c.get_name()), out);
        }
        return;
    }
    out.push(json!({
        "name": path,
        "about": command.get_about().map(ToString::to_string),
        "json": COMMANDS.contains(&path),
        "args": args(command),
    }));
}

fn args(command: &clap::Command) -> Vec<Value> {
    command
        .get_arguments()
        .map(|a| {
            json!({
                "name": a.get_id().as_str(),
                "flag": a.get_long().map(|l| format!("--{l}")),
                "required": a.is_required_set(),
                "takes_value": a.get_action().takes_values(),
                "env": a.get_env().map(|e| e.to_string_lossy()),
                "help": a.get_help().map(ToString::to_string),
            })
        })
        .collect()
}
