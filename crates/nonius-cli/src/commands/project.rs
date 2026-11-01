//! Проект целиком: `init`, `part new`, `list`, `info`, `overview`, `modules`.

use std::path::PathBuf;

use anyhow::Result;
use nonius_app::{create_project, App, History, ItemState, ItemStatus};
use nonius_core::extension::Registry;
use nonius_core::meta::ItemKind;

use crate::format::{kind_word, short_date};

/// `nonius init`: папка — `path`, иначе `-C`, иначе текущая.
pub fn init(path: Option<&PathBuf>, name: Option<&str>) -> Result<()> {
    let root = match path {
        Some(p) => p.clone(),
        None => std::env::current_dir()?,
    };
    let created = create_project(&root, name)?;
    println!(
        "Создан проект «{}» в {}",
        created.name,
        created.root.display()
    );
    match created.history {
        History::Lfs => println!("История версий включена (git + LFS)."),
        History::NoLfs => println!(
            "предупреждение: Git LFS не найден — модели будут храниться прямо в git. \
             Установите Git LFS (https://git-lfs.com)."
        ),
        History::Off(e) => println!("предупреждение: история версий не включена: {e}"),
    }
    Ok(())
}

/// `nonius part new`.
pub fn part_new(app: &App, name: &str, assembly: bool) -> Result<()> {
    let kind = if assembly {
        ItemKind::Assembly
    } else {
        ItemKind::Part
    };
    let item = app.new_item(name, kind)?;
    println!("Добавлено: {} ({})", item.name, kind_word(kind));
    Ok(())
}

/// `nonius list`.
pub fn list(app: &App) -> Result<()> {
    let items = app.item_list()?;
    if items.is_empty() {
        println!("Пока пусто. Добавьте деталь: nonius part new <имя>");
    }
    for i in items {
        println!("{:<10} {}", kind_word(i.kind), i.name);
    }
    Ok(())
}

/// `nonius info`.
pub fn info(app: &App) -> Result<()> {
    let p = app.project();
    let c = p.config();
    let items = app.item_list()?;
    let parts = items.iter().filter(|i| i.kind == ItemKind::Part).count();
    println!("Проект:    {}", c.name);
    println!("Папка:     {}", p.root().display());
    println!("Деталей:   {parts}");
    println!("Сборок:    {}", items.len() - parts);
    println!("Допуск:    {} мм", c.compare.tolerance_mm);
    for m in &c.members {
        println!("Участник:  {} ({:?})", m.name, m.role);
    }
    Ok(())
}

/// `nonius overview`: сверху итог, дальше одна деталь — одна строка.
pub fn overview(app: &App, changed: bool) -> Result<()> {
    let items = app.items()?;
    let count = |state| items.iter().filter(|s| s.state() == state).count();
    let parts = items
        .iter()
        .filter(|s| s.item.kind == ItemKind::Part)
        .count();
    println!(
        "Проект «{}»: деталей {parts}, сборок {}. Сохранено {}, изменено {}, новых {}.",
        app.project().config().name,
        items.len() - parts,
        count(ItemState::Saved),
        count(ItemState::Changed),
        count(ItemState::New),
    );
    if items.is_empty() {
        println!("Пока пусто. Добавьте деталь: nonius part new <имя>");
    }
    for s in &items {
        if !changed || s.state() != ItemState::Saved {
            println!("{}", overview_line(s));
        }
    }
    Ok(())
}

fn overview_line(s: &ItemStatus) -> String {
    let mut line = format!("{} | {}", s.item.name, kind_word(s.item.kind));
    if let Some(v) = &s.latest {
        line.push_str(&format!(
            " | {} из {} | {} | {} | {}",
            v.version,
            s.versions,
            short_date(&v.date),
            v.signature(),
            v.summary()
        ));
    }
    match s.state() {
        ItemState::New => line.push_str(" | новая"),
        ItemState::Saved => line.push_str(" | сохранена"),
        ItemState::Changed => {
            line.push_str(&format!(" | изменено файлов: {}", s.changes.len()));
        }
    }
    line
}

/// `nonius modules`.
pub fn modules(registry: &Registry) {
    for m in registry.manifests() {
        println!("{:<16} {} {} — {:?}", m.id, m.version, m.name, m.points);
    }
    match nonius_occt::occt_version() {
        Some(v) => println!("Геометрия: OpenCascade {v}"),
        None => println!("Геометрия: нет (программа собрана без OpenCascade)"),
    }
}
