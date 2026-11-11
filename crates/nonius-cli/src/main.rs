//! `nonius` — командная строка и окно Nonius.
//!
//! Тонкая обёртка: разбирает команду, вызывает действие из `nonius-app` и
//! печатает итог. Те же действия вызывают окно и сервер MCP для ИИ.
//!
//! Файлы: здесь — запуск и выбор команды, набор модулей; `args.rs` — команды
//! и справка, `commands/` — печать итогов и ответ в JSON, `format.rs` — мелочи вывода,
//! `window.rs` — окно (`nonius` без команды и `nonius view`),
//! `window_lock.rs` — один проект в одном окне,
//! `welcome.rs` — подсказка, если окно не открылось.

mod args;
mod commands;
mod format;
mod welcome;
mod window;
mod window_lock;

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use nonius_app::{AiActor, App, GitOptions};
use nonius_core::extension::Registry;
use nonius_step::StepHeaderSource;

use args::{Cli, Command, FileCommand, FolderCommand, PartCommand};
use commands::{
    compare, describe, files, history, import, intake, json, layout, mcp, passport, project,
};

fn main() -> ExitCode {
    // Без команды — скорее всего, двойной щелчок в Проводнике: открыть окно.
    if std::env::args_os().len() <= 1 {
        return window::double_click();
    }
    let wants_json = std::env::args_os().any(|a| a == "--json");
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) if wants_json => return json::usage(e),
        Err(e) => e.exit(),
    };
    if cli.json {
        return json::main(&cli);
    }
    let in_window = matches!(cli.command, Command::View { .. });
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ошибка: {e:#}");
            if in_window {
                // У окна, запущенного двойным щелчком, консоли нет.
                nonius_gui::show_error(&format!("{e:#}"));
            }
            ExitCode::FAILURE
        }
    }
}

/// Все модули, которые идут вместе с программой.
fn registry() -> Registry {
    registry_with(nonius_occt::register)
}

/// Модули для окна: без картинки `changes.png`, окну нужна только 3D-сцена.
fn window_registry() -> Registry {
    registry_with(nonius_occt::register_for_window)
}

fn registry_with(occt: fn(&mut Registry) -> nonius_core::Result<()>) -> Registry {
    let mut r = Registry::with_builtins();
    // Геометрия OpenCascade (если программа собрана с ним) встаёт первой,
    // заголовок STEP — запасной вариант.
    occt(&mut r).expect("встроенный модуль должен проходить проверку");
    r.add_geometry_source(Box::new(StepHeaderSource::new()))
        .expect("встроенный модуль должен проходить проверку");
    // STEP из файлов SolidWorks — только там, где SolidWorks установлен.
    // В тестах не подключаем: они не должны запускать чужую программу.
    #[cfg(not(test))]
    nonius_solidworks::register(&mut r).expect("встроенный модуль должен проходить проверку");
    r
}

/// Открыть проект: из `-C` или ближайший к текущей папке.
fn open_app(cli: &Cli) -> Result<App> {
    let start = match &cli.project {
        Some(p) => p.clone(),
        None => std::env::current_dir().context("не удалось узнать текущую папку")?,
    };
    let ai = cli
        .ai
        .as_deref()
        .map(|name| AiActor::new(name, cli.ai_model.as_deref()))
        .transpose()?;
    let git = GitOptions { ai, identity: None };
    Ok(App::open(&start, registry())?.with_git_options(git))
}

fn run(cli: Cli) -> Result<()> {
    let app = || open_app(&cli);

    match &cli.command {
        Command::Init { path, name } => {
            project::init(path.as_ref().or(cli.project.as_ref()), name.as_deref())?
        }
        Command::Part(PartCommand::Move { name, folder }) => {
            layout::part_move(&app()?, name, folder)?
        }
        Command::Folder(FolderCommand::New { path }) => layout::folder_new(&app()?, path)?,
        Command::Map => layout::map(&app()?)?,
        Command::Upgrade => layout::upgrade(&app()?)?,
        Command::Part(PartCommand::New {
            name,
            assembly,
            folder,
        }) => project::part_new(&app()?, name, *assembly, folder.as_deref())?,
        Command::Part(PartCommand::Rename { name, new }) => {
            project::part_rename(&app()?, name, new)?
        }
        Command::Part(PartCommand::Delete { name }) => project::part_delete(&app()?, name)?,
        Command::Part(PartCommand::Deleted) => project::part_deleted(&app()?)?,
        Command::Part(PartCommand::Recover { name }) => project::part_recover(&app()?, name)?,
        Command::List => project::list(&app()?)?,
        Command::Info => project::info(&app()?)?,
        Command::Overview { changed } => project::overview(&app()?, *changed)?,
        Command::Modules => project::modules(&registry()),
        Command::Agents => project::agents(&app()?)?,
        Command::Mcp { write } => mcp::serve(&cli, *write)?,
        Command::Describe => println!("{}", describe::describe()),
        Command::Import { file, part } => import::import(&app()?, file, part.as_deref())?,
        Command::Intake {
            folder,
            apply,
            quick,
            deep,
            ask,
            yes,
            no,
        } => {
            let options = intake::Options {
                apply: *apply,
                check: intake::check(*quick, *deep),
                ask: *ask,
                yes,
                no,
            };
            intake::intake(&app()?, folder, &options)?;
        }
        Command::Status { part } => history::status(&app()?, part.as_deref())?,
        Command::Commit { part, message } => history::commit(&app()?, part, message)?,
        Command::Log { part } => history::log(&app()?, part)?,
        Command::Diff { part, from, to } => {
            compare::diff(&app()?, part, from.as_deref(), to.as_deref())?
        }
        Command::Compare {
            part,
            version,
            file,
            with,
        } => {
            let other = compare::other(file.as_deref(), with.as_deref())?;
            compare::compare(&app()?, part, version.as_deref(), other)?
        }
        Command::Hide { part, version } => history::hide(&app()?, part, version, true)?,
        Command::Unhide { part, version } => history::hide(&app()?, part, version, false)?,
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
        Command::Files {
            part,
            from,
            to,
            text,
        } => files::files(
            &app()?,
            part,
            from.as_deref(),
            to.as_deref(),
            text.as_deref(),
        )?,
        Command::Show {
            part,
            file,
            version,
        } => files::show(&app()?, part, file, version.as_deref())?,
        Command::File(FileCommand::Add { part, file }) => files::add(&app()?, part, file)?,
        Command::File(FileCommand::Remove { part, file }) => files::remove(&app()?, part, file)?,
        Command::Open {
            part,
            file,
            version,
            programs,
            with,
            folder,
        } => {
            if *programs {
                files::programs(file);
            } else {
                let way = files::way(file, with.as_deref(), *folder)?;
                files::open(&app()?, part, file, version.as_deref(), &way)?;
            }
        }
        Command::View { part, from, to } => window::open(
            cli.project.as_deref(),
            part.as_deref(),
            from.as_deref(),
            to.as_deref(),
        )?,
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
