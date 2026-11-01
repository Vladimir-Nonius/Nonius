//! Файлы детали: `files`, `open`.

use anyhow::Result;
use nonius_app::{App, DiffLineKind, FileChangeKind};

use crate::format::parse_version;

/// `nonius files`.
pub fn files(
    app: &App,
    part: &str,
    from: Option<&str>,
    to: Option<&str>,
    text: Option<&str>,
) -> Result<()> {
    let item = app.item(part)?;
    let from = match from {
        Some(v) => Some(parse_version(v)?),
        None => app.latest(&item)?.map(|v| v.version),
    };
    let to = to.map(parse_version).transpose()?;
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

/// `nonius open`.
pub fn open(app: &App, part: &str, file: &str, version: Option<&str>) -> Result<()> {
    let version = version.map(parse_version).transpose()?;
    let opened = app.open_file(part, version, file)?;
    println!("Открыт {}", opened.display());
    if version.is_some() {
        println!("Это копия: правки в ней не попадут в историю.");
    }
    Ok(())
}
