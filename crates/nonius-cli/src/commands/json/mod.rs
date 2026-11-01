//! Ответ для программ: `--json`. Успех — `{"ok":true,"data":…}`, ошибка —
//! `{"ok":false,"error":{"code":…,"message":…}}`, всё в stdout. Коды ошибок
//! постоянны: программа смотрит на код, а сообщение показывает человеку.
//! Выход: 0 — успех, 1 — ошибка действия, 2 — неверная команда.

use std::process::ExitCode;

use anyhow::Result;
use nonius_app::{App, CompareOptions, ItemState, NoProgress, StoreError, MATERIALS};
use nonius_core::meta::ItemKind;
use serde_json::{json, Value};

use crate::args::{Cli, Command, PartCommand};
use crate::commands::{compare, describe, files, passport, project};
use crate::format::parse_version;

mod view;

use view::{change, item, kind, mass, version};

/// Команды, которые отвечают в JSON. Остальные открывают окна или программы.
pub const COMMANDS: &[&str] = &[
    "part new",
    "list",
    "info",
    "overview",
    "agents",
    "modules",
    "import",
    "status",
    "commit",
    "log",
    "diff",
    "restore",
    "export",
    "files",
    "material",
    "materials",
    "describe",
];

/// Выполнить команду и напечатать ответ.
pub fn main(cli: &Cli) -> ExitCode {
    let out = answer(cli);
    println!("{out}");
    if out["ok"] == json!(true) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Выполнить команду и вернуть ответ. Здесь же работает сервер MCP.
pub fn answer(cli: &Cli) -> Value {
    match data(cli) {
        Ok(Some(data)) => json!({ "ok": true, "data": data }),
        Ok(None) => failure("no_json", "эта команда не отвечает в JSON"),
        Err(e) => failure(code(&e), &format!("{e:#}")),
    }
}

/// Команду не удалось разобрать.
pub fn usage(error: clap::Error) -> ExitCode {
    use clap::error::ErrorKind;
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        error.exit();
    }
    println!("{}", failure("usage", error.to_string().trim()));
    ExitCode::from(2)
}

/// Ответ об ошибке.
pub fn failure(code: &str, message: &str) -> Value {
    let error = json!({ "code": code, "message": message });
    json!({ "ok": false, "error": error })
}

/// Постоянный код ошибки; `failed` — у ошибки нет своего кода.
fn code(error: &anyhow::Error) -> &'static str {
    error
        .chain()
        .find_map(|e| {
            e.downcast_ref::<nonius_app::Error>()
                .map(nonius_app::Error::code)
                .or_else(|| e.downcast_ref::<StoreError>().map(StoreError::code))
                .or_else(|| {
                    e.downcast_ref::<nonius_core::Error>()
                        .map(nonius_core::Error::code)
                })
        })
        .unwrap_or("failed")
}

fn data(cli: &Cli) -> Result<Option<Value>> {
    let app = || crate::open_app(cli);
    Ok(Some(match &cli.command {
        Command::Part(PartCommand::New { name, assembly }) => {
            let item = app()?.new_item(name, project::kind_of(*assembly))?;
            json!({ "name": item.name.as_str(), "kind": kind(item.kind) })
        }
        Command::List => {
            let list = app()?.item_list()?;
            let items: Vec<Value> = list
                .iter()
                .map(|i| json!({ "name": i.name.as_str(), "kind": kind(i.kind) }))
                .collect();
            json!({ "items": items })
        }
        Command::Info => info(&app()?)?,
        Command::Overview { changed } => overview(&app()?, *changed)?,
        Command::Agents => json!({ "updated": app()?.update_agents()? }),
        Command::Modules => modules(),
        Command::Import { file, part } => {
            let done = app()?.import(file, part.as_deref())?;
            json!({
                "item": done.item.name.as_str(),
                "target": done.target,
                "preview": done.preview.is_some(),
                "warnings": done.warnings,
                "meta": done.meta,
                "mass": mass(&done.meta),
            })
        }
        Command::Status { part } => {
            let app = app()?;
            let items = match part {
                Some(p) => vec![app.item_status(p)?],
                None => app.items()?,
            };
            let changed = items.iter().filter(|s| !s.changes.is_empty());
            json!({ "items": changed.map(item).collect::<Vec<_>>() })
        }
        Command::Commit { part, message } => version(&app()?.save_version(part, message)?),
        Command::Log { part } => {
            let versions = app()?.versions(part)?;
            json!({ "versions": versions.iter().map(version).collect::<Vec<_>>() })
        }
        Command::Diff { part, from, to } => {
            let (from, to) = compare::versions(from.as_deref(), to.as_deref())?;
            let c = app()?.compare(part, from, to, &CompareOptions::default(), &NoProgress)?;
            let files: Vec<Value> = c
                .file_changes
                .iter()
                .map(|f| json!({ "path": f.path, "change": change(f.kind) }))
                .collect();
            json!({
                "item": c.item.as_str(),
                "from": c.from_label(),
                "to": c.to.map(|v| v.to_string()),
                "reports": c.reports,
                "files": files,
                "empty": c.is_empty(),
            })
        }
        Command::Restore {
            part,
            version: number,
            message,
        } => version(&app()?.restore(part, parse_version(number)?, message.as_deref())?),
        Command::Export {
            part,
            version: number,
            dest,
        } => {
            let (v, exported) = app()?.export(part, parse_version(number)?, dest)?;
            json!({ "version": version(&v), "dest": dest, "files": exported })
        }
        Command::Files {
            part,
            from,
            to,
            text,
        } => {
            let app = app()?;
            let (from, to) = files::range(&app, part, from.as_deref(), to.as_deref())?;
            let labels = json!({
                "from": from.map(|v| v.to_string()),
                "to": to.map(|v| v.to_string()),
            });
            match text {
                Some(path) => text_diff(&app, part, from, to, path, labels)?,
                None => file_list(&app, part, from, to, labels)?,
            }
        }
        Command::Material {
            part,
            material,
            density,
            remove,
        } => {
            let done = passport::apply(&app()?, part, material.as_deref(), *density, *remove)?;
            json!({
                "item": done.item.as_str(),
                "meta": done.meta,
                "mass": mass(&done.meta),
            })
        }
        Command::Materials => materials(),
        Command::Describe => describe::describe(),
        Command::Init { .. }
        | Command::Render { .. }
        | Command::View { .. }
        | Command::Open { .. }
        | Command::Mcp { .. } => return Ok(None),
    }))
}

fn info(app: &App) -> Result<Value> {
    let p = app.project();
    let list = app.item_list()?;
    let parts = list.iter().filter(|i| i.kind == ItemKind::Part).count();
    Ok(json!({
        "name": p.config().name,
        "root": p.root(),
        "parts": parts,
        "assemblies": list.len() - parts,
        "tolerance_mm": p.config().compare.tolerance_mm,
    }))
}

fn overview(app: &App, changed: bool) -> Result<Value> {
    let items = app.items()?;
    let count = |state| items.iter().filter(|s| s.state() == state).count();
    let shown = items
        .iter()
        .filter(|s| !changed || s.state() != ItemState::Saved);
    Ok(json!({
        "project": app.project().config().name,
        "saved": count(ItemState::Saved),
        "changed": count(ItemState::Changed),
        "new": count(ItemState::New),
        "agents_outdated": app.project().agents_outdated(),
        "items": shown.map(item).collect::<Vec<_>>(),
    }))
}

fn modules() -> Value {
    let list: Vec<Value> = crate::registry()
        .manifests()
        .into_iter()
        .map(
            |m| json!({ "id": m.id.to_string(), "version": m.version.to_string(), "name": m.name }),
        )
        .collect();
    json!({ "modules": list, "opencascade": nonius_occt::occt_version() })
}

fn materials() -> Value {
    let list: Vec<Value> = MATERIALS
        .iter()
        .map(|m| json!({ "name": m.name, "density_kg_m3": m.density_kg_m3, "aliases": m.aliases }))
        .collect();
    json!({ "materials": list })
}

fn text_diff(
    app: &App,
    part: &str,
    from: Option<nonius_core::VersionNumber>,
    to: Option<nonius_core::VersionNumber>,
    path: &str,
    mut out: Value,
) -> Result<Value> {
    use nonius_app::DiffLineKind;
    let lines: Vec<Value> = app
        .file_diff(part, from, to, path)?
        .iter()
        .map(|l| {
            let mark = match l.kind {
                DiffLineKind::Same => "same",
                DiffLineKind::Added => "added",
                DiffLineKind::Removed => "removed",
                DiffLineKind::Gap => "gap",
            };
            json!({ "kind": mark, "text": l.text })
        })
        .collect();
    out["path"] = json!(path);
    out["lines"] = json!(lines);
    Ok(out)
}

fn file_list(
    app: &App,
    part: &str,
    from: Option<nonius_core::VersionNumber>,
    to: Option<nonius_core::VersionNumber>,
    mut out: Value,
) -> Result<Value> {
    let list: Vec<Value> = app
        .version_files(part, from, to)?
        .iter()
        .map(|f| {
            json!({
                "path": f.path,
                "change": f.change.map(change),
                "before": f.before,
                "after": f.after,
            })
        })
        .collect();
    out["files"] = json!(list);
    Ok(out)
}
