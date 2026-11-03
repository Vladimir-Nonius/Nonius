//! Проект целиком: `init`, `part new`, `list`, `info`, `overview`, `agents`,
//! `modules`.

use std::path::{Path, PathBuf};

use anyhow::Result;
use nonius_app::{create_project, App, Created, History, ItemState, ItemStatus};
use nonius_core::extension::Registry;
use nonius_core::meta::ItemKind;

use crate::format::{kind_word, short_date};

/// `nonius init`: папка — `path`, иначе `-C`, иначе текущая.
pub fn init(path: Option<&PathBuf>, name: Option<&str>) -> Result<()> {
    let created = create(path, name)?;
    println!(
        "Создан проект «{}» в {}",
        created.name,
        created.root.display()
    );
    match history_words(&created.history) {
        (_, Some(warning)) => println!("предупреждение: {warning}"),
        (_, None) => println!("История версий включена (git + LFS)."),
    }
    Ok(())
}

/// Создать проект в папке `path`, по умолчанию — в текущей.
pub fn create(path: Option<&PathBuf>, name: Option<&str>) -> Result<Created> {
    let root = match path {
        Some(p) => p.clone(),
        None => std::env::current_dir()?,
    };
    Ok(create_project(&root, name)?)
}

/// Как ведётся история: слово для программ и предупреждение для человека.
pub fn history_words(history: &History) -> (&'static str, Option<String>) {
    match history {
        History::Lfs => ("lfs", None),
        History::NoLfs => (
            "git",
            Some(
                "Git LFS не найден — модели будут храниться прямо в git. \
                 Установите Git LFS (https://git-lfs.com)."
                    .into(),
            ),
        ),
        History::Off(e) => ("off", Some(format!("история версий не включена: {e}"))),
    }
}

/// Сборка или деталь по ключу `--assembly`.
pub fn kind_of(assembly: bool) -> ItemKind {
    if assembly {
        ItemKind::Assembly
    } else {
        ItemKind::Part
    }
}

/// `nonius part new`.
pub fn part_new(app: &App, name: &str, assembly: bool, folder: Option<&Path>) -> Result<()> {
    let kind = kind_of(assembly);
    let item = app.new_item_in(name, kind, folder.unwrap_or(Path::new("")))?;
    println!("Добавлено: {} ({})", item.name, kind_word(kind));
    Ok(())
}

/// `nonius part rename`.
pub fn part_rename(app: &App, name: &str, new: &str) -> Result<()> {
    let item = app.rename_item(name, new)?;
    println!("«{name}» теперь «{}», версии остались при ней", item.name);
    Ok(())
}

/// `nonius part delete`.
pub fn part_delete(app: &App, name: &str) -> Result<()> {
    app.delete_item(name)?;
    println!("Удалено: {name}. Вернуть: nonius part recover {name}");
    Ok(())
}

/// `nonius part deleted`.
pub fn part_deleted(app: &App) -> Result<()> {
    let list = app.deleted_items()?;
    if list.is_empty() {
        println!("Удалённых деталей нет.");
    }
    for d in list {
        println!("{}  {}  {}", short_date(&d.date), d.author, d.name);
    }
    Ok(())
}

/// `nonius part recover`.
pub fn part_recover(app: &App, name: &str) -> Result<()> {
    let item = app.recover_item(name)?;
    println!("Возвращено: {}", item.name);
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
    if p.is_old_layout() {
        println!(
            "Формат:    старый (parts/, assemblies/), только просмотр. Обновить: nonius upgrade"
        );
    }
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
    if app.project().agents_outdated() {
        println!("Правила для ИИ в AGENTS.md устарели: nonius agents");
    }
    Ok(())
}

/// `nonius agents`.
pub fn agents(app: &App) -> Result<()> {
    if app.update_agents()? {
        println!("AGENTS.md обновлён и зафиксирован.");
    } else {
        println!("AGENTS.md уже свежий.");
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
