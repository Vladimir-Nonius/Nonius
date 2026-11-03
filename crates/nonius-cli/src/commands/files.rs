//! Файлы детали: `files`, `show`, `file add`, `file remove`, `open`.

use std::path::Path;

use anyhow::{bail, Result};
use nonius_app::{programs_for, App, DiffLineKind, FileChangeKind, OpenWay};
use nonius_core::VersionNumber;

use crate::format::parse_version;

/// Какие версии показать: `from` по умолчанию последняя.
pub fn range(
    app: &App,
    part: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<(Option<VersionNumber>, Option<VersionNumber>)> {
    let from = match from {
        Some(v) => Some(parse_version(v)?),
        None => app.latest(&app.item(part)?)?.map(|v| v.version),
    };
    Ok((from, to.map(parse_version).transpose()?))
}

/// `nonius files`.
pub fn files(
    app: &App,
    part: &str,
    from: Option<&str>,
    to: Option<&str>,
    text: Option<&str>,
) -> Result<()> {
    let (from, to) = range(app, part, from, to)?;
    if let Some(path) = text {
        for line in app.file_diff(part, from, to, path)? {
            let mark = match line.kind {
                DiffLineKind::Same => " ",
                DiffLineKind::Added => "+",
                DiffLineKind::Removed => "-",
                DiffLineKind::Gap => "…",
            };
            println!("{mark} {}", line.text);
        }
        return Ok(());
    }
    for f in app.version_files(part, from, to)? {
        let word = match f.change {
            Some(FileChangeKind::Added) => "новый",
            Some(FileChangeKind::Modified) => "изменён",
            Some(FileChangeKind::Deleted) => "удалён",
            None => "",
        };
        let size = match (f.change, f.before, f.after) {
            (Some(FileChangeKind::Modified), Some(b), Some(a)) => format!("{b} → {a} байт"),
            (_, b, a) => format!("{} байт", a.or(b).unwrap_or(0)),
        };
        println!("{word:<8} {}  ({size})", f.path);
    }
    Ok(())
}

/// `nonius show`.
pub fn show(app: &App, part: &str, file: &str, version: Option<&str>) -> Result<()> {
    let version = version.map(parse_version).transpose()?;
    println!("{}", app.file_text(part, version, file)?);
    Ok(())
}

/// `nonius file add`.
pub fn add(app: &App, part: &str, file: &Path) -> Result<()> {
    let path = app.add_file(part, file)?;
    println!("Добавлен {path} в «{part}». Сохраните версию, чтобы он попал в историю.");
    Ok(())
}

/// `nonius file remove`.
pub fn remove(app: &App, part: &str, file: &str) -> Result<()> {
    app.remove_file(part, file)?;
    println!("Убран {file} из «{part}». В сохранённых версиях он остаётся.");
    Ok(())
}

/// `nonius open`.
pub fn open(app: &App, part: &str, file: &str, version: Option<&str>, way: &OpenWay) -> Result<()> {
    let version = version.map(parse_version).transpose()?;
    let opened = app.open_file(part, version, file, way)?;
    println!("Открыт {}", opened.display());
    if version.is_some() {
        println!("Это копия: правки в ней не попадут в историю.");
    }
    Ok(())
}

/// `nonius open --programs`.
pub fn programs(file: &str) {
    let found = programs_for(file);
    if found.is_empty() {
        println!("Система не знает программ для этого файла.");
    }
    for program in found {
        println!("{}", program.name);
    }
}

/// Чем открыть файл по ключам `--with` и `--folder`.
pub fn way(file: &str, with: Option<&str>, folder: bool) -> Result<OpenWay> {
    if folder {
        return Ok(OpenWay::Folder);
    }
    let Some(name) = with else {
        return Ok(OpenWay::Default);
    };
    match programs_for(file).into_iter().find(|p| p.name == name) {
        Some(program) => Ok(OpenWay::Program(program)),
        None => bail!("программы «{name}» нет в списке: nonius open … --programs"),
    }
}
