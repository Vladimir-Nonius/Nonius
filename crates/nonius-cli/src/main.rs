//! `nonius` — командная строка Nonius.
//!
//! Тот же набор действий потом будет в веб-интерфейсе и в MCP для ИИ.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nonius_core::extension::Registry;
use nonius_core::meta::ItemKind;
use nonius_core::{PartName, Project};

/// Nonius — контроль версий для CAD-моделей.
#[derive(Parser)]
#[command(name = "nonius", version, about, long_about = None)]
struct Cli {
    /// Папка проекта (по умолчанию — текущая или ближайшая выше).
    #[arg(short = 'C', long, global = true)]
    project: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Создать проект в папке.
    Init {
        /// Папка (по умолчанию текущая).
        path: Option<PathBuf>,
        /// Название проекта (по умолчанию имя папки).
        #[arg(long)]
        name: Option<String>,
    },
    /// Детали и сборки.
    #[command(subcommand)]
    Part(PartCommand),
    /// Список деталей и сборок.
    List,
    /// Сведения о проекте.
    Info,
    /// Подключённые модули.
    Modules,
}

#[derive(Subcommand)]
enum PartCommand {
    /// Добавить деталь (или сборку с --assembly).
    New {
        /// Имя детали: кириллица можно, / \ : * ? " < > | @ # [ ] ^ нельзя.
        name: String,
        /// Это сборка.
        #[arg(long)]
        assembly: bool,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ошибка: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let open = || -> Result<Project> {
        let start = match &cli.project {
            Some(p) => p.clone(),
            None => std::env::current_dir().context("не удалось узнать текущую папку")?,
        };
        Ok(Project::discover(&start)?)
    };

    match &cli.command {
        Command::Init { path, name } => {
            let root = match path.as_ref().or(cli.project.as_ref()) {
                Some(p) => p.clone(),
                None => std::env::current_dir()?,
            };
            let name = name.clone().unwrap_or_else(|| {
                root.canonicalize()
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "Проект".into())
            });
            let p = Project::init(&root, &name)?;
            println!(
                "Создан проект «{}» в {}",
                p.config().name,
                p.root().display()
            );
        }
        Command::Part(PartCommand::New { name, assembly }) => {
            let p = open()?;
            let kind = if *assembly {
                ItemKind::Assembly
            } else {
                ItemKind::Part
            };
            let item = p.create_item(kind, &PartName::new(name.as_str())?)?;
            println!("Добавлено: {} ({})", item.name, kind_word(kind));
        }
        Command::List => {
            let p = open()?;
            let items = p.items()?;
            if items.is_empty() {
                println!("Пока пусто. Добавьте деталь: nonius part new <имя>");
            }
            for i in items {
                println!("{:<10} {}", kind_word(i.kind), i.name);
            }
        }
        Command::Info => {
            let p = open()?;
            let c = p.config();
            let items = p.items()?;
            let parts = items.iter().filter(|i| i.kind == ItemKind::Part).count();
            println!("Проект:    {}", c.name);
            println!("Папка:     {}", p.root().display());
            println!("Деталей:   {parts}");
            println!("Сборок:    {}", items.len() - parts);
            println!("Допуск:    {} мм", c.compare.tolerance_mm);
            for m in &c.members {
                println!("Участник:  {} ({:?})", m.name, m.role);
            }
        }
        Command::Modules => {
            for m in Registry::with_builtins().manifests() {
                println!("{:<16} {} {} — {:?}", m.id, m.version, m.name, m.points);
            }
        }
    }
    Ok(())
}

fn kind_word(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Part => "деталь",
        ItemKind::Assembly => "сборка",
    }
}
