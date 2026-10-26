//! `nonius` — командная строка Nonius.
//!
//! Тот же набор действий потом будет в веб-интерфейсе и в MCP для ИИ.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use nonius_core::compare::{ChangeKind, ChangeReport, CompareContext};
use nonius_core::extension::{GeometrySource, Registry, Snapshot};
use nonius_core::meta::ItemKind;
use nonius_core::project::{Item, META_FILE};
use nonius_core::{schema, PartName, Project, VersionNumber};
use nonius_step::StepHeaderSource;
use nonius_store::{FileChangeKind, GitOptions, Store, VersionInfo};

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
    /// Добавить файл модели (STEP, SLDPRT, SLDASM) в деталь.
    Import {
        /// Файл модели.
        file: PathBuf,
        /// Деталь (по умолчанию — имя файла без расширения; создаётся, если нет).
        #[arg(long)]
        part: Option<String>,
    },
    /// Что изменилось с последней версии.
    Status {
        /// Деталь (по умолчанию — все).
        part: Option<String>,
    },
    /// Зафиксировать новую версию детали.
    Commit {
        /// Деталь.
        part: String,
        /// Описание: что и почему изменено.
        #[arg(short, long)]
        message: String,
    },
    /// История версий детали.
    Log {
        /// Деталь.
        part: String,
    },
    /// Сравнить версии: по умолчанию последнюю с текущим состоянием.
    Diff {
        /// Деталь.
        part: String,
        /// С какой версии (по умолчанию последняя).
        from: Option<String>,
        /// По какую версию (по умолчанию текущее состояние).
        to: Option<String>,
    },
    /// Вернуть деталь к старой версии (создаётся новая версия, история сохраняется).
    Restore {
        /// Деталь.
        part: String,
        /// Версия: v3 или 3.
        version: String,
        /// Почему откатываем.
        #[arg(short, long)]
        message: Option<String>,
    },
    /// Нарисовать изменения цветом: зелёное — добавлено, красное — убрано.
    Render {
        /// Деталь.
        part: String,
        /// С какой версии (по умолчанию последняя).
        from: Option<String>,
        /// По какую версию (по умолчанию текущее состояние).
        to: Option<String>,
        /// Куда сохранить картинку (по умолчанию <деталь>-<было>-<стало>.png).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Выгрузить файлы версии в папку.
    Export {
        /// Деталь.
        part: String,
        /// Версия: v3 или 3.
        version: String,
        /// Куда выгрузить.
        dest: PathBuf,
    },
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
            match Store::open(p, &GitOptions::default()) {
                Ok(s) if !s.lfs_enabled() => println!(
                    "предупреждение: Git LFS не найден — модели будут храниться прямо в git. \
                     Установите Git LFS (https://git-lfs.com)."
                ),
                Ok(_) => println!("История версий включена (git + LFS)."),
                Err(e) => println!("предупреждение: история версий не включена: {e}"),
            }
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
            for m in registry().manifests() {
                println!("{:<16} {} {} — {:?}", m.id, m.version, m.name, m.points);
            }
            match nonius_occt::occt_version() {
                Some(v) => println!("Геометрия: OpenCascade {v}"),
                None => println!("Геометрия: нет (программа собрана без OpenCascade)"),
            }
        }
        Command::Import { file, part } => import(&open()?, file, part.as_deref())?,
        Command::Status { part } => status(&store()?, part.as_deref())?,
        Command::Commit { part, message } => {
            let s = store()?;
            let item = s.item(part)?;
            let v = s.commit(&item, message)?;
            println!("Зафиксирована {} {}: {}", item.name, v.version, v.summary());
        }
        Command::Log { part } => {
            let s = store()?;
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
        }
        Command::Diff { part, from, to } => diff(&store()?, part, from.as_deref(), to.as_deref())?,
        Command::Render {
            part,
            from,
            to,
            output,
        } => render_changes(
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
        } => {
            let s = store()?;
            let item = s.item(part)?;
            let v = s.restore(&item, parse_version(version)?, message.as_deref())?;
            println!(
                "Деталь «{}» возвращена: {} — {}",
                item.name,
                v.version,
                v.summary()
            );
        }
        Command::Export {
            part,
            version,
            dest,
        } => {
            let s = store()?;
            let item = s.item(part)?;
            let v = s.version(&item, parse_version(version)?)?;
            let files = s.export(&item, &v, dest)?;
            println!(
                "Выгружено {} файлов версии {} в {}",
                files.len(),
                v.version,
                dest.display()
            );
        }
    }
    Ok(())
}

fn import(project: &Project, file: &Path, part: Option<&str>) -> Result<()> {
    if !file.is_file() {
        bail!("{}: файл не найден", file.display());
    }
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let (target, kind) = match ext.as_str() {
        "step" | "stp" => ("model.step", ItemKind::Part),
        "sldprt" => ("model.sldprt", ItemKind::Part),
        "sldasm" => ("model.sldasm", ItemKind::Assembly),
        _ => bail!(
            "{}: неизвестный формат (поддерживаются STEP, SLDPRT, SLDASM)",
            file.display()
        ),
    };
    let name = match part {
        Some(p) => p.to_string(),
        None => file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    let name = PartName::new(name.as_str())
        .with_context(|| "имя детали из имени файла не подходит, задайте его: --part <имя>")?;

    let item = match project.find_item(&name)? {
        Some(i) => i,
        None => project.create_item(kind, &name)?,
    };
    let dest = item.dir.join(target);
    fs::copy(file, &dest)
        .with_context(|| format!("не удалось скопировать в {}", dest.display()))?;

    let mut meta = project.load_meta(&item)?;
    if let Some(src) = registry().geometry_source_for(file) {
        // Геометрию прочитать не удалось (пустой или повреждённый STEP) —
        // импорт не ломаем: берём сведения из заголовка и предупреждаем.
        let read = match src.read(file) {
            Ok(m) => m,
            Err(e) if src.manifest().id != StepHeaderSource::ID => {
                println!("предупреждение: геометрия не прочитана: {e}");
                StepHeaderSource::new().read(file)?
            }
            Err(e) => return Err(e.into()),
        };
        meta.geometry = read.geometry;
        meta.source = read.source;
        if read.material.is_some() {
            meta.material = read.material;
        }
        meta.properties.extend(read.properties);
    } else if meta.source.cad.is_none() {
        meta.source.cad = Some("SolidWorks".into());
    }
    schema::save(&meta, &item.dir.join(META_FILE))?;

    println!("Импортировано в «{}»: {target}", item.name);
    if let Some(cad) = &meta.source.cad {
        println!("Источник: {cad}");
    }
    if let Some(v) = meta.geometry.volume_mm3 {
        println!("Объём: {v:.1} мм³");
    }
    if target == "model.step" {
        match nonius_occt::preview_png(&dest, &item.dir.join("preview.png")) {
            Ok(true) => println!("Превью: preview.png"),
            Ok(false) => {}
            Err(e) => println!("предупреждение: превью не сделано: {e}"),
        }
    }
    println!(
        "Зафиксируйте версию: nonius commit {} -m \"что изменилось\"",
        item.name
    );
    Ok(())
}

fn status(store: &Store, part: Option<&str>) -> Result<()> {
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

fn diff(store: &Store, part: &str, from: Option<&str>, to: Option<&str>) -> Result<()> {
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

fn render_changes(
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

fn marker(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Added => "+",
        ChangeKind::Removed => "-",
        ChangeKind::Modified => "~",
        ChangeKind::Moved => "→",
        ChangeKind::Renamed => "✎",
    }
}

/// `v3` или `3` → номер версии.
fn parse_version(s: &str) -> Result<VersionNumber> {
    let s = s.trim();
    let text = if s.starts_with('v') {
        s.to_string()
    } else {
        format!("v{s}")
    };
    Ok(text.parse()?)
}

/// `2026-10-26T12:34:56+03:00` → `2026-10-26 12:34`.
fn short_date(iso: &str) -> String {
    iso.get(..16).unwrap_or(iso).replacen('T', " ", 1)
}

fn kind_word(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Part => "деталь",
        ItemKind::Assembly => "сборка",
    }
}
