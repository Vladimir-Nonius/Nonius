//! `nonius intake`: разобрать папку с моделями и чертежами в проект.
//!
//! Без `--apply` только показывает план — ничего не меняет. Версии, угаданные
//! по именам, проверяются формой деталей; `--quick` — без этой проверки,
//! `--deep` — посмотреть форму всех деталей.

use std::path::Path;

use anyhow::Result;
use nonius_app::{
    App, IntakeDone, IntakePlan, PlannedItem, Progress, ShapeCheck, ShapeVerdict, NOT_OURS,
};
use nonius_core::meta::ItemKind;

/// Ход разбора — строками в консоль.
struct Printed;

impl Progress for Printed {
    fn step(&self, what: &str) {
        println!("{what}");
    }
    fn cancelled(&self) -> bool {
        false
    }
}

pub fn intake(app: &App, folder: &Path, apply: bool, check: ShapeCheck) -> Result<()> {
    let plan = app.intake_plan_with(folder, check, &Printed)?;
    print_plan(&plan);
    if plan.items.is_empty() {
        return Ok(());
    }
    if !apply {
        println!();
        println!(
            "Это только план, ничего не изменено. Разложить: nonius intake \"{}\" --apply",
            folder.display()
        );
        return Ok(());
    }
    println!();
    let done = app.intake_apply(&plan, &Printed)?;
    print_done(&done);
    Ok(())
}

/// Глубина проверки формой по флагам `--quick` и `--deep`.
pub fn check(quick: bool, deep: bool) -> ShapeCheck {
    match (quick, deep) {
        (true, _) => ShapeCheck::Off,
        (_, true) => ShapeCheck::All,
        _ => ShapeCheck::Names,
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Стоит ли показать деталь человеку: версии угаданы по именам и форма их
/// не подтвердила, либо форма развела похожие имена на разные детали.
fn unsure(item: &PlannedItem) -> bool {
    match item.shape {
        Some(ShapeVerdict::Confirmed) => false,
        Some(ShapeVerdict::Copy | ShapeVerdict::Different) => true,
        None => item.guessed,
    }
}

fn print_plan(plan: &IntakePlan) {
    let assemblies = plan
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Assembly)
        .count();
    let docs: usize = plan.items.iter().map(|i| i.docs.len()).sum();
    println!(
        "Папка {}: деталей — {}, сборок — {}, чертежей и бумаг при них — {}, прочих файлов — {}",
        plan.source.display(),
        plan.items.len() - assemblies,
        assemblies,
        docs,
        plan.left.len()
    );
    let mut folder = None;
    for item in &plan.items {
        if folder != Some(&item.folder) {
            folder = Some(&item.folder);
            println!();
            if item.folder.is_empty() {
                println!("В корень проекта:");
            } else {
                println!("В папку «{}»:", item.folder);
            }
        }
        let kind = if item.kind == ItemKind::Assembly {
            "сборка"
        } else {
            "деталь"
        };
        let versions = if item.versions.len() > 1 {
            format!(", версий: {}", item.versions.len())
        } else {
            String::new()
        };
        let mark = if unsure(item) { "  (?)" } else { "" };
        println!("  {kind} «{}»{versions}{mark}", item.name);
        for why in &item.why {
            println!("      {why}");
        }
        for doc in &item.docs {
            println!("      + {}", file_name(doc));
        }
    }
    if plan.items.iter().any(unsure) {
        println!();
        println!("(?) — догадка, которую форма деталей не подтвердила: проверьте.");
    }
    let (junk, papers): (Vec<_>, Vec<_>) = plan.left.iter().partition(|l| l.why == NOT_OURS);
    if !papers.is_empty() {
        println!();
        println!("Останутся на месте:");
        for left in papers {
            println!("  {} — {}", file_name(&left.path), left.why);
        }
    }
    if !junk.is_empty() {
        println!();
        println!(
            "Ещё {} файлов — не модели и не чертежи (расчёты, логи и прочее): их не трогаю.",
            junk.len()
        );
    }
}

fn print_done(done: &IntakeDone) {
    let versions: u32 = done.saved.iter().map(|(_, n)| n).sum();
    println!();
    println!(
        "Разложено: {}, версий сохранено: {versions}",
        done.saved.len()
    );
    for w in &done.warnings {
        println!("предупреждение: {w}");
    }
    for f in &done.failed {
        println!("не вышло: «{}» — {}", f.name, f.why);
    }
    if done.stopped {
        println!("Остановлено: остальное не тронуто.");
    }
}
