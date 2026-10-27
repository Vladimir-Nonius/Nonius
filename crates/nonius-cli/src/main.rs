//! `nonius` — командная строка Nonius.
//!
//! Тот же набор действий потом будет в веб-интерфейсе и в MCP для ИИ.
//!
//! Файлы: здесь — запуск и выбор команды, `args.rs` — команды и справка,
//! `commands/` — выполнение команд, `format.rs` — мелочи вывода,
//! `welcome.rs` — подсказка при двойном щелчке.

mod args;
mod commands;
mod format;
mod welcome;

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use nonius_core::extension::Registry;
use nonius_core::Project;
use nonius_step::StepHeaderSource;
use nonius_store::{GitOptions, Store};

use args::{Cli, Command, PartCommand};
use commands::{compare, history, import, project};

fn main() -> ExitCode {
    // Без команды — скорее всего, двойной щелчок в Проводнике.
    if std::env::args_os().len() <= 1 {
        welcome::welcome();
        return ExitCode::SUCCESS;
    }
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ошибка: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Все модули, которые идут вместе с программой.
fn registry() -> Registry {
    let mut r = Registry::with_builtins();
    // Геометрия OpenCascade (если программа собрана с ним) встаёт первой,
    // заголовок STEP — запасной вариант.
    nonius_occt::register(&mut r).expect("встроенный модуль должен проходить проверку");
    r.add_geometry_source(Box::new(StepHeaderSource::new()))
        .expect("встроенный модуль должен проходить проверку");
    r
}

fn run(cli: Cli) -> Result<()> {
    let open = || -> Result<Project> {
        let start = match &cli.project {
            Some(p) => p.clone(),
            None => std::env::current_dir().context("не удалось узнать текущую папку")?,
        };
        Ok(Project::discover(&start)?)
    };
    let store = || -> Result<Store> { Ok(Store::open(open()?, &GitOptions::default())?) };

    match &cli.command {
        Command::Init { path, name } => {
            project::init(path.as_ref().or(cli.project.as_ref()), name.as_ref())?
        }
        Command::Part(PartCommand::New { name, assembly }) => {
            project::part_new(&open()?, name, *assembly)?
        }
        Command::List => project::list(&open()?)?,
        Command::Info => project::info(&open()?)?,
        Command::Modules => project::modules(),
        Command::Import { file, part } => import::import(&open()?, file, part.as_deref())?,
        Command::Status { part } => history::status(&store()?, part.as_deref())?,
        Command::Commit { part, message } => history::commit(&store()?, part, message)?,
        Command::Log { part } => history::log(&store()?, part)?,
        Command::Diff { part, from, to } => {
            compare::diff(&store()?, part, from.as_deref(), to.as_deref())?
        }
        Command::Render {
            part,
            from,
            to,
            output,
        } => compare::render_changes(
            &store()?,
            part,
            from.as_deref(),
            to.as_deref(),
            output.as_deref(),
        )?,
        Command::Restore {
            part,
            version,
            message,
        } => history::restore(&store()?, part, version, message.as_deref())?,
        Command::Export {
            part,
            version,
            dest,
        } => history::export(&store()?, part, version, dest)?,
    }
    Ok(())
}
