//! `nonius mcp`: сервер MCP для ИИ на stdin и stdout. Своих действий здесь нет:
//! вызов инструмента превращается в обычную команду с ответом `--json`, а список
//! инструментов строится из разбора команд. Без `--write` сервер только читает.
//! Всё, что сделано через MCP, подписано: это всегда ИИ.

use std::io::{self, BufRead, Write};

use anyhow::Result;
use clap::{CommandFactory, Parser};
use nonius_app::AiActor;
use serde_json::{json, Map, Value};

use crate::args::Cli;
use crate::commands::json::{self, COMMANDS};

/// Версии протокола, которые понимает сервер; первая — своя.
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// Команды, которые что-то меняют. `material` меняет, только если назначает.
const WRITING: [&str; 14] = [
    "init",
    "part new",
    "part rename",
    "part delete",
    "part recover",
    "import",
    "commit",
    "restore",
    "export",
    "agents",
    "file add",
    "file remove",
    "hide",
    "unhide",
];

/// Подпись, если клиент не назвал себя.
const UNKNOWN_AI: &str = "MCP";

struct Session {
    project: Option<String>,
    ai: Option<String>,
    model: Option<String>,
    write: bool,
}

/// Отвечать на сообщения, пока клиент не закроет stdin.
pub fn serve(cli: &Cli, write: bool) -> Result<()> {
    let mut session = Session {
        project: cli.project.as_ref().map(|p| p.display().to_string()),
        ai: cli.ai.clone(),
        model: cli.ai_model.clone(),
        write,
    };
    let mut out = io::stdout();
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => session.handle(&message),
            Err(e) => Some(failure(&Value::Null, -32700, &e.to_string())),
        };
        if let Some(reply) = reply {
            writeln!(out, "{reply}")?;
            out.flush()?;
        }
    }
    Ok(())
}

fn success(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn failure(id: &Value, code: i32, message: &str) -> Value {
    let error = json!({ "code": code, "message": message });
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

impl Session {
    /// Ответ на сообщение; у уведомлений без `id` ответа нет.
    fn handle(&mut self, message: &Value) -> Option<Value> {
        let id = message.get("id")?;
        let params = &message["params"];
        Some(match message["method"].as_str().unwrap_or_default() {
            "initialize" => success(id, self.initialize(params)),
            "ping" => success(id, json!({})),
            "tools/list" => success(id, json!({ "tools": self.tools() })),
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or_default();
                let empty = Map::new();
                let args = params["arguments"].as_object().unwrap_or(&empty);
                success(id, reply(&self.call(name, args)))
            }
            other => failure(id, -32601, &format!("нет метода {other}")),
        })
    }

    fn initialize(&mut self, params: &Value) -> Value {
        let client = params["clientInfo"]["name"].as_str().unwrap_or_default();
        let valid = AiActor::new(client, None).is_ok();
        if self.ai.is_none() && valid {
            self.ai = Some(client.to_string());
        }
        let asked = params["protocolVersion"].as_str().unwrap_or_default();
        let version = PROTOCOLS
            .into_iter()
            .find(|p| *p == asked)
            .unwrap_or(PROTOCOLS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "nonius", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "Начните с инструмента overview. Файлы моделей не читайте.",
        })
    }

    fn tools(&self) -> Vec<Value> {
        COMMANDS
            .iter()
            .filter(|path| self.write || !WRITING.contains(path))
            .filter_map(|path| tool(path))
            .collect()
    }

    fn call(&self, name: &str, args: &Map<String, Value>) -> Value {
        let path = COMMANDS.iter().find(|p| tool_name(p) == name);
        let Some(path) = path.filter(|p| self.write || !WRITING.contains(p)) else {
            return json::failure("unknown_tool", &format!("нет инструмента {name}"));
        };
        let assigns = args.contains_key("material") || args.get("remove") == Some(&json!(true));
        if !self.write && *path == "material" && assigns {
            return json::failure("read_only", "сервер запущен без --write: только чтение");
        }
        let Some(command) = find(path) else {
            return json::failure("unknown_tool", &format!("нет инструмента {name}"));
        };
        let argv = match self.argv(path, &command, args) {
            Ok(argv) => argv,
            Err(message) => return json::failure("usage", &message),
        };
        match Cli::try_parse_from(argv) {
            Ok(cli) => json::answer(&cli),
            Err(e) => json::failure("usage", e.to_string().trim()),
        }
    }

    /// Вызов как командная строка. Значения идут через `=` и после `--`,
    /// поэтому значение от ИИ не может стать ключом.
    fn argv(
        &self,
        path: &str,
        command: &clap::Command,
        args: &Map<String, Value>,
    ) -> Result<Vec<String>, String> {
        let mut argv = vec!["nonius".to_string()];
        if let Some(project) = &self.project {
            argv.push(format!("--project={project}"));
        }
        let ai = self.ai.as_deref().unwrap_or(UNKNOWN_AI);
        argv.push(format!("--ai={ai}"));
        if let Some(model) = &self.model {
            argv.push(format!("--ai-model={model}"));
        }
        argv.extend(path.split(' ').map(String::from));
        if let Some(name) = args.keys().find(|k| !known(command, k)) {
            return Err(format!("нет параметра {name}"));
        }

        let mut positional = Vec::new();
        let mut gap = None;
        for arg in command.get_arguments() {
            let id = arg.get_id().as_str();
            let value = args.get(id);
            match (arg.get_long(), value) {
                (_, None | Some(Value::Null)) => {
                    if arg.is_positional() {
                        gap = gap.or(Some(id));
                    }
                }
                (Some(long), Some(Value::Bool(flag))) if !arg.get_action().takes_values() => {
                    if *flag {
                        argv.push(format!("--{long}"));
                    }
                }
                (Some(long), Some(value)) => argv.push(format!("--{long}={}", text(id, value)?)),
                (None, Some(value)) => {
                    if let Some(missing) = gap {
                        return Err(format!("{id} задан, а {missing} нет"));
                    }
                    positional.push(text(id, value)?);
                }
            }
        }
        if !positional.is_empty() {
            argv.push("--".to_string());
            argv.extend(positional);
        }
        Ok(argv)
    }
}

/// Ответ инструмента: тот же JSON, что у `--json`.
fn reply(answer: &Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": answer.to_string() }],
        "isError": answer["ok"] != json!(true),
    })
}

fn known(command: &clap::Command, name: &str) -> bool {
    command.get_arguments().any(|a| a.get_id().as_str() == name)
}

fn text(id: &str, value: &Value) -> Result<String, String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(format!("{id}: нужна строка или число")),
    }
}

fn tool_name(path: &str) -> String {
    path.replace(' ', "_")
}

fn find(path: &str) -> Option<clap::Command> {
    let root = Cli::command();
    let mut found = &root;
    for name in path.split(' ') {
        found = found.find_subcommand(name)?;
    }
    Some(found.clone())
}

/// Описание инструмента из описания команды.
fn tool(path: &str) -> Option<Value> {
    let command = find(path)?;
    let mut properties = Map::new();
    let mut required = Vec::new();
    for arg in command.get_arguments() {
        let id = arg.get_id().as_str();
        let kind = if arg.get_action().takes_values() {
            json!(["string", "number"])
        } else {
            json!("boolean")
        };
        let help = arg.get_help().map(ToString::to_string);
        properties.insert(id.to_string(), json!({ "type": kind, "description": help }));
        if arg.is_required_set() {
            required.push(id.to_string());
        }
    }
    Some(json!({
        "name": tool_name(path),
        "description": command.get_about().map(ToString::to_string),
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        },
    }))
}
