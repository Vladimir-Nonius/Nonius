//! Проект целиком: `init`, `part new`, `list`, `info`, `modules`.

use std::path::PathBuf;

use anyhow::Result;
use nonius_app::{create_project, App, History};
use nonius_core::extension::Registry;
use nonius_core::meta::ItemKind;

use crate::format::kind_word;

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
