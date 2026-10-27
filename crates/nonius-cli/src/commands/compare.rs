//! `nonius diff` и `nonius render`: сравнение версий и цветная картинка.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use nonius_core::compare::{ChangeReport, CompareContext};
use nonius_core::extension::Snapshot;
use nonius_core::project::Item;
use nonius_store::{FileChangeKind, Store, VersionInfo};

use crate::format::{marker, parse_version};
use crate::registry;

pub fn diff(store: &Store, part: &str, from: Option<&str>, to: Option<&str>) -> Result<()> {
    let item = store.item(part)?;
    let (before, after, from_label, after_label) = snapshots(store, &item, from, to)?;
    println!("«{}»: {from_label} → {after_label}", item.name);
    let ctx = CompareContext {
        tolerance_mm: store.project().config().compare.tolerance_mm,
        ..CompareContext::default()
    };
    let mut any = false;
    for report in registry().compare_all(&before, &after, &ctx)? {
        for c in &report.changes {
            any = true;
            println!("  {} {c}", marker(c.kind));
        }
    }
    if to.is_none() {
        for c in store.changes(&item)? {
            any = true;
            let word = match c.kind {
                FileChangeKind::Added => "+ файл",
                FileChangeKind::Modified => "~ файл",
                FileChangeKind::Deleted => "- файл",
            };
            println!("  {word} {}", c.path);
        }
    }
    if !any {
        println!("  изменений нет");
    }
    Ok(())
}

/// Пара снимков для сравнения: `from` (по умолчанию последняя версия) и
/// `to` (по умолчанию текущее состояние). Возвращает подписи версий.
fn snapshots(
    store: &Store,
    item: &Item,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<(Snapshot, Snapshot, String, String)> {
    let pick = |v: &str| -> Result<VersionInfo> { Ok(store.version(item, parse_version(v)?)?) };
    let before_info = match from {
        Some(v) => pick(v)?,
        None => match store.latest(item)? {
            Some(v) => v,
            None => bail!("у «{}» ещё нет версий — сравнивать не с чем", item.name),
        },
    };
    let before = store.snapshot(item, &before_info)?;
    let (after, after_label) = match to {
        Some(v) => {
            let info = pick(v)?;
            (store.snapshot(item, &info)?, info.version.to_string())
        }
        None => (store.working_snapshot(item)?, "текущее".to_string()),
    };
    Ok((before, after, before_info.version.to_string(), after_label))
}

pub fn render_changes(
    store: &Store,
    part: &str,
    from: Option<&str>,
    to: Option<&str>,
    output: Option<&Path>,
) -> Result<()> {
    let item = store.item(part)?;
    let reg = registry();
    let Some(renderer) = reg.renderers().next() else {
        bail!("цветная подсветка доступна в сборке Nonius с OpenCascade");
    };
    let (before, after, from_label, to_label) = snapshots(store, &item, from, to)?;
    let work = store
        .project()
        .root()
        .join(".git")
        .join("nonius")
        .join("render");
    let files = renderer.render(&before, &after, &ChangeReport::empty(""), &work)?;
    let Some(image) = files.first() else {
        bail!("у «{}» нет model.step — рисовать нечего", item.name);
    };
    let target = match output {
        Some(p) => p.to_path_buf(),
        None => PathBuf::from(format!("{}-{from_label}-{to_label}.png", item.name)),
    };
    fs::copy(image, &target)
        .with_context(|| format!("не удалось сохранить {}", target.display()))?;
    println!("«{}»: {from_label} → {to_label}", item.name);
    println!("Картинка: {}", target.display());
    println!("Зелёное — добавлено, красное — убрано, серое — без изменений.");
    Ok(())
}
