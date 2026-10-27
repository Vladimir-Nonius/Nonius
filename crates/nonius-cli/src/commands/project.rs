//! Проект целиком: `init`, `part new`, `list`, `info`, `modules`.

use std::path::PathBuf;

use anyhow::Result;
use nonius_core::meta::ItemKind;
use nonius_core::{PartName, Project};
use nonius_store::{GitOptions, Store};

use crate::format::kind_word;
use crate::registry;

/// `nonius init`: папка — `path`, иначе `-C`, иначе текущая.
pub fn init(path: Option<&PathBuf>, name: Option<&String>) -> Result<()> {
    let root = match path {
        Some(p) => p.clone(),
        None => std::env::current_dir()?,
    };
    let name = name.cloned().unwrap_or_else(|| {
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
    Ok(())
}

/// `nonius part new`.
pub fn part_new(p: &Project, name: &str, assembly: bool) -> Result<()> {
    let kind = if assembly {
        ItemKind::Assembly
    } else {
        ItemKind::Part
    };
    let item = p.create_item(kind, &PartName::new(name)?)?;
    println!("Добавлено: {} ({})", item.name, kind_word(kind));
    Ok(())
}

/// `nonius list`.
pub fn list(p: &Project) -> Result<()> {
    let items = p.items()?;
    if items.is_empty() {
        println!("Пока пусто. Добавьте деталь: nonius part new <имя>");
    }
    for i in items {
        println!("{:<10} {}", kind_word(i.kind), i.name);
    }
    Ok(())
}

/// `nonius info`.
pub fn info(p: &Project) -> Result<()> {
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
    Ok(())
}

/// `nonius modules`.
pub fn modules() {
    for m in registry().manifests() {
        println!("{:<16} {} {} — {:?}", m.id, m.version, m.name, m.points);
    }
    match nonius_occt::occt_version() {
        Some(v) => println!("Геометрия: OpenCascade {v}"),
        None => println!("Геометрия: нет (программа собрана без OpenCascade)"),
    }
}
