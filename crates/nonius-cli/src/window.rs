//! Окно программы: `nonius` без команды (двойной щелчок) и `nonius view`.
//! Мост от общего слоя `nonius-app` к окну — `window/source.rs`.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use crate::format::parse_version;
use crate::welcome;

use anyhow::{Context, Result};
use nonius_gui::source::Pick;

mod source;

use source::{plainly, AppSource, ThisMachine};

/// Запуск без команды. В Windows окно запускается отдельным процессом без
/// чёрного окна консоли, а этот процесс сразу выходит. `NONIUS_NO_WINDOW=1`
/// (серверы, проверки) — только подсказка в консоли.
pub fn double_click() -> ExitCode {
    if std::env::var_os("NONIUS_NO_WINDOW").is_some_and(|v| v == "1") {
        welcome::welcome();
        return ExitCode::SUCCESS;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if let Ok(exe) = std::env::current_exe() {
            let child = std::process::Command::new(exe)
                .arg("view")
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
            if child.is_ok() {
                return ExitCode::SUCCESS;
            }
        }
    }
    match open(None, None, None, None) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ошибка: {e:#}\n");
            welcome::welcome();
            ExitCode::FAILURE
        }
    }
}

/// `nonius view`: открыть окно, сразу показать деталь и версии, если заданы.
pub fn open(
    project: Option<&Path>,
    part: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<()> {
    let start = match project {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir().context("не удалось узнать текущую папку")?,
    };
    // Не в папке проекта — окно откроет последний проект или начальный экран.
    let source = AppSource::open(&start).ok();
    let pick = |v: Option<&str>| -> Result<Option<Pick>> {
        v.map(|v| Ok(Pick::Version(parse_version(v)?.get())))
            .transpose()
    };
    let picks = match (pick(from)?, pick(to)?) {
        (None, None) => None,
        (a, b) => Some((a.unwrap_or(Pick::Current), b.unwrap_or(Pick::Current))),
    };
    nonius_gui::run(nonius_gui::Options {
        project: source,
        opener: Arc::new(|p: &Path| AppSource::open(p).map_err(plainly)),
        creator: Arc::new(|p: &Path| AppSource::create(p).map_err(plainly)),
        machine: Arc::new(ThisMachine),
        chooser: Arc::new(nonius_gui::SystemChooser),
        part: part.map(str::to_string),
        picks,
        drafts: nonius_gui::drafts_dir(),
        mover: Arc::new(|from: &Path, to: &Path| {
            nonius_app::move_project(from, to).map_err(|e| plainly(e.into()))
        }),
    })
}

#[cfg(test)]
mod scenarios;
