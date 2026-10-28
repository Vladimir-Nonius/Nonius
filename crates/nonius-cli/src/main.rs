//! `nonius` — командная строка Nonius.
//!
//! Тонкая обёртка: разбирает команду, вызывает действие из `nonius-app` и
//! печатает итог. Те же действия вызывают окно и (позже) MCP для ИИ.
//!
//! Файлы: здесь — запуск и выбор команды, набор модулей; `args.rs` — команды
//! и справка, `commands/` — печать итогов, `format.rs` — мелочи вывода,
//! `welcome.rs` — подсказка при двойном щелчке.

mod args;
mod commands;
mod format;
mod welcome;

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use nonius_app::App;
use nonius_core::extension::Registry;
use nonius_step::StepHeaderSource;

use args::{Cli, Command, PartCommand};
use commands::{compare, history, import, passport, project};

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
    let app = || -> Result<App> {
        let start = match &cli.project {
            Some(p) => p.clone(),
            None => std::env::current_dir().context("не удалось узнать текущую папку")?,
        };
        Ok(App::open(&start, registry())?)
    };

    match &cli.command {
        Command::Init { path, name } => {
            project::init(path.as_ref().or(cli.project.as_ref()), name.as_deref())?
        }
        Command::Part(PartCommand::New { name, assembly }) => {
            project::part_new(&app()?, name, *assembly)?
        }
        Command::List => project::list(&app()?)?,
        Command::Info => project::info(&app()?)?,
        Command::Modules => project::modules(&registry()),
        Command::Import { file, part } => import::import(&app()?, file, part.as_deref())?,
        Command::Status { part } => history::status(&app()?, part.as_deref())?,
        Command::Commit { part, message } => history::commit(&app()?, part, message)?,
        Command::Log { part } => history::log(&app()?, part)?,
        Command::Diff { part, from, to } => {
            compare::diff(&app()?, part, from.as_deref(), to.as_deref())?
        }
        Command::Render {
            part,
            from,
            to,
            output,
        } => compare::render_changes(
            &app()?,
            part,
            from.as_deref(),
            to.as_deref(),
            output.as_deref(),
        )?,
        Command::Restore {
            part,
            version,
            message,
        } => history::restore(&app()?, part, version, message.as_deref())?,
        Command::Export {
            part,
            version,
            dest,
        } => history::export(&app()?, part, version, dest)?,
        Command::Material {
            part,
            material,
            density,
            remove,
        } => passport::material(&app()?, part, material.as_deref(), *density, *remove)?,
        Command::Materials => passport::materials(),
    }
    Ok(())
}
