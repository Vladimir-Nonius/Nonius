//! Папки проекта: `map`, `folder new`, `part move`, `upgrade` — для человека
//! и в JSON.

use std::path::Path;

use anyhow::Result;
use nonius_app::{App, ProjectMap};
use serde_json::{json, Value};

use crate::commands::json::view::kind;
use crate::format::kind_word;

/// `nonius map`: папки и детали деревом, как на диске.
pub fn map(app: &App) -> Result<()> {
    let map = app.map()?;
    if map.folders.is_empty() && map.items.is_empty() {
        println!("Пока пусто. Добавьте деталь: nonius part new <имя>");
    }
    for (path, line) in lines(&map) {
        let depth = path.matches('/').count();
        println!("{}{line}", "  ".repeat(depth));
    }
    if app.is_old_layout() {
        println!("\nПроект старого формата, менять его нельзя. Обновить: nonius upgrade");
    }
    Ok(())
}

/// Строки карты по порядку пути: папка, потом её содержимое.
fn lines(map: &ProjectMap) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = map
        .folders
        .iter()
        .map(|f| (f.clone(), format!("{}/", last(f))))
        .chain(map.items.iter().map(|i| {
            (
                i.path.clone(),
                format!("{} ({})", i.name, kind_word(i.kind)),
            )
        }))
        .collect();
    out.sort_by(|a, b| a.0.split('/').cmp(b.0.split('/')));
    out
}

fn last(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `nonius map --json`.
pub fn map_json(app: &App) -> Result<Value> {
    let map = app.map()?;
    let items: Vec<Value> = map
        .items
        .iter()
        .map(|i| json!({ "name": i.name.as_str(), "kind": kind(i.kind), "path": i.path }))
        .collect();
    Ok(json!({
        "old_layout": app.is_old_layout(),
        "folders": map.folders,
        "items": items,
    }))
}

/// `nonius folder new`.
pub fn folder_new(app: &App, folder: &Path) -> Result<()> {
    app.new_folder(folder)?;
    println!("Папка создана: {}", folder.display());
    Ok(())
}

/// `nonius part move`.
pub fn part_move(app: &App, name: &str, folder: &Path) -> Result<()> {
    let item = app.move_item(name, folder)?;
    let place = item
        .dir
        .parent()
        .and_then(|p| p.strip_prefix(app.project().root()).ok())
        .filter(|p| !p.as_os_str().is_empty())
        .map_or("корне проекта".into(), |p| {
            let parts: Vec<_> = p.iter().map(|c| c.to_string_lossy()).collect();
            format!("«{}»", parts.join("/"))
        });
    println!("«{name}» теперь в {place}, версии остались при ней");
    Ok(())
}

/// `nonius upgrade`.
pub fn upgrade(app: &App) -> Result<()> {
    if !app.is_old_layout() {
        println!("Проект уже на папках, обновлять нечего.");
        return Ok(());
    }
    app.upgrade()?;
    println!(
        "Проект обновлён: детали и сборки перенесены в корень, версии остались при них.\n\
         Разложить их по папкам: nonius folder new <папка>, nonius part move <деталь> <папка>"
    );
    Ok(())
}
