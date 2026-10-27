//! История детали: `status`, `commit`, `log`, `restore`, `export`.

use std::path::Path;

use anyhow::Result;
use nonius_core::project::Item;
use nonius_store::{FileChangeKind, Store};

use crate::format::{parse_version, short_date};

pub fn status(store: &Store, part: Option<&str>) -> Result<()> {
    let items: Vec<Item> = match part {
        Some(p) => vec![store.item(p)?],
        None => store.project().items()?,
    };
    let mut clean = 0;
    for item in &items {
        let changes = store.changes(item)?;
        if changes.is_empty() {
            clean += 1;
            if part.is_some() {
                println!("«{}»: изменений нет", item.name);
            }
            continue;
        }
        match store.latest(item)? {
            Some(v) => println!("«{}» (последняя {}):", item.name, v.version),
            None => println!("«{}» (ещё нет версий):", item.name),
        }
        for c in changes {
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
pub fn commit(s: &Store, part: &str, message: &str) -> Result<()> {
    let item = s.item(part)?;
    let v = s.commit(&item, message)?;
    println!("Зафиксирована {} {}: {}", item.name, v.version, v.summary());
    Ok(())
}

/// `nonius log`.
pub fn log(s: &Store, part: &str) -> Result<()> {
    let item = s.item(part)?;
    let versions = s.versions(&item)?;
    if versions.is_empty() {
        println!(
            "У «{}» ещё нет версий: nonius commit {} -m \"...\"",
            item.name, item.name
        );
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
pub fn restore(s: &Store, part: &str, version: &str, message: Option<&str>) -> Result<()> {
    let item = s.item(part)?;
    let v = s.restore(&item, parse_version(version)?, message)?;
    println!(
        "Деталь «{}» возвращена: {} — {}",
        item.name,
        v.version,
        v.summary()
    );
    Ok(())
}

/// `nonius export`.
pub fn export(s: &Store, part: &str, version: &str, dest: &Path) -> Result<()> {
    let item = s.item(part)?;
    let v = s.version(&item, parse_version(version)?)?;
    let files = s.export(&item, &v, dest)?;
    println!(
        "Выгружено {} файлов версии {} в {}",
        files.len(),
        v.version,
        dest.display()
    );
    Ok(())
}
