//! `nonius diff` и `nonius render`: сравнение версий и цветная картинка.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use nonius_app::{App, CompareOptions, FileChangeKind, NoProgress, Other, Outside};
use nonius_core::VersionNumber;

use crate::format::{marker, parse_version};

pub fn versions(
    from: Option<&str>,
    to: Option<&str>,
) -> Result<(Option<VersionNumber>, Option<VersionNumber>)> {
    Ok((
        from.map(parse_version).transpose()?,
        to.map(parse_version).transpose()?,
    ))
}

pub fn diff(app: &App, part: &str, from: Option<&str>, to: Option<&str>) -> Result<()> {
    let (from, to) = versions(from, to)?;
    let c = app.compare(part, from, to, &CompareOptions::default(), &NoProgress)?;
    println!("«{}»: {} → {}", c.item, c.from_label(), c.to_label());
    for report in &c.reports {
        for ch in &report.changes {
            println!("  {} {ch}", marker(ch.kind));
        }
    }
    for f in &c.file_changes {
        let word = match f.kind {
            FileChangeKind::Added => "+ файл",
            FileChangeKind::Modified => "~ файл",
            FileChangeKind::Deleted => "- файл",
        };
        println!("  {word} {}", f.path);
    }
    if c.is_empty() {
        println!("  изменений нет");
    }
    Ok(())
}

/// С чем сравнить: файл или другая деталь.
pub fn other<'a>(file: Option<&'a Path>, with: Option<&'a str>) -> Result<Other<'a>> {
    match (file, with) {
        (Some(path), _) => Ok(Other::File(path)),
        (None, Some(part)) => Ok(Other::Part(part)),
        (None, None) => bail!("укажите --file или --with"),
    }
}

/// Сравнить с файлом или деталью, только числа.
pub fn outside(app: &App, part: &str, version: Option<&str>, other: Other) -> Result<Outside> {
    let at = version.map(parse_version).transpose()?;
    Ok(app.compare_outside(part, at, other, &CompareOptions::default(), &NoProgress)?)
}

/// `nonius compare`.
pub fn compare(app: &App, part: &str, version: Option<&str>, other: Other) -> Result<()> {
    let done = outside(app, part, version, other)?;
    let changes: Vec<_> = done.reports.iter().flat_map(|r| &r.changes).collect();
    for ch in &changes {
        println!("  {} {ch}", marker(ch.kind));
    }
    if changes.is_empty() {
        println!("  изменений нет");
    }
    Ok(())
}

pub fn render_changes(
    app: &App,
    part: &str,
    from: Option<&str>,
    to: Option<&str>,
    output: Option<&Path>,
) -> Result<()> {
    let (from, to) = versions(from, to)?;
    // Только картинка: числа здесь не печатаются, а считать форму дважды долго.
    let options = CompareOptions {
        reports: false,
        render: true,
    };
    let c = app.compare(part, from, to, &options, &NoProgress)?;
    let Some(image) = c.file("changes.png") else {
        bail!("у «{}» нет model.step — рисовать нечего", c.item);
    };
    let target = match output {
        Some(p) => p.to_path_buf(),
        None => PathBuf::from(format!(
            "{}-{}-{}.png",
            c.item,
            c.from_label(),
            c.to_label()
        )),
    };
    fs::copy(image, &target)
        .with_context(|| format!("не удалось сохранить {}", target.display()))?;
    println!("«{}»: {} → {}", c.item, c.from_label(), c.to_label());
    println!("Картинка: {}", target.display());
    println!("Зелёное — добавлено, красное — убрано, серое — без изменений.");
    Ok(())
}
