//! История детали: `status`, `commit`, `log`, `restore`, `export`.

use std::path::Path;

use anyhow::Result;
use nonius_app::{App, FileChangeKind};

use crate::format::{parse_version, short_date};

pub fn status(app: &App, part: Option<&str>) -> Result<()> {
    let items = match part {
        Some(p) => vec![app.item(p)?],
        None => app.item_list()?,
    };
    let mut clean = 0;
    for item in &items {
        // История нужна только изменённым деталям: так быстрее на больших проектах.
        let changes = app.changes(item)?;
        if changes.is_empty() {
            clean += 1;
            if part.is_some() {
                println!("«{}»: изменений нет", item.name);
            }
            continue;
        }
        match app.latest(item)? {
            Some(v) => println!("«{}» (последняя {}):", item.name, v.version),
            None => println!("«{}» (ещё нет версий):", item.name),
        }
        for c in &changes {
            let word = match c.kind {
                FileChangeKind::Added => "новый",
                FileChangeKind::Modified => "изменён",
                FileChangeKind::Deleted => "удалён",
            };
            println!("  {word:<8} {}", c.path);
        }
    }
    if part.is_none() {
        if items.is_empty() {
            println!("Пока нет деталей.");
        } else if clean == items.len() {
            println!("Все детали зафиксированы, изменений нет.");
        }
    }
    Ok(())
}

/// `nonius commit`.
pub fn commit(app: &App, part: &str, message: &str) -> Result<()> {
    let v = app.save_version(part, message)?;
    println!("Зафиксирована {} {}: {}", v.item, v.version, v.summary());
    Ok(())
}

/// `nonius log`.
pub fn log(app: &App, part: &str) -> Result<()> {
    let versions = app.versions(part)?;
    if versions.is_empty() {
        println!("У «{part}» ещё нет версий: nonius commit {part} -m \"...\"");
    }
    for v in versions {
        let ver = v.version.to_string();
        println!(
            "{ver:<5} {}  {}  {}",
            short_date(&v.date),
            v.author,
            v.summary()
        );
    }
    Ok(())
}

/// `nonius restore`.
pub fn restore(app: &App, part: &str, version: &str, message: Option<&str>) -> Result<()> {
    let v = app.restore(part, parse_version(version)?, message)?;
    println!(
        "Деталь «{}» возвращена: {} — {}",
        v.item,
        v.version,
        v.summary()
    );
    Ok(())
}

/// `nonius export`.
pub fn export(app: &App, part: &str, version: &str, dest: &Path) -> Result<()> {
    let (v, files) = app.export(part, parse_version(version)?, dest)?;
    println!(
        "Выгружено {} файлов версии {} в {}",
        files.len(),
        v.version,
        dest.display()
    );
    Ok(())
}
