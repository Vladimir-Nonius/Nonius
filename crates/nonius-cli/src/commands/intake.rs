//! `nonius intake`: разобрать папку с моделями и чертежами в проект.
//!
//! Без `--apply` только показывает план — ничего не меняет. Версии, угаданные
//! по именам, проверяются формой деталей; `--quick` — без этой проверки,
//! `--deep` — посмотреть форму всех деталей.
//!
//! Где программа не уверена, в плане стоят вопросы. `--ask` задаёт их по
//! одному прямо в консоли; `--yes ID` и `--no ID` отвечают заранее (так
//! отвечают окно и ИИ). После каждого ответа план пересчитывается.

use std::path::Path;

use anyhow::Result;
use nonius_app::{
    App, IntakeDone, IntakePlan, PlannedItem, Progress, Question, ShapeCheck, ShapeVerdict,
    NOT_OURS,
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

/// Как разбирать: флаги команды.
pub struct Options<'a> {
    /// Выполнить план.
    pub apply: bool,
    /// Глубина проверки формой.
    pub check: ShapeCheck,
    /// Задать вопросы по одному в консоли.
    pub ask: bool,
    /// Вопросы, на которые ответ «да» и «нет» (их `id`).
    pub yes: &'a [String],
    pub no: &'a [String],
}

/// Ответы из флагов `--yes` и `--no`. В одном значении можно дать несколько
/// `id`, каждый с новой строки: так их передаёт ИИ.
pub fn answers(yes: &[String], no: &[String]) -> Vec<(String, bool)> {
    let ids = |list: &[String], answer: bool| -> Vec<(String, bool)> {
        list.iter()
            .flat_map(|v| v.lines())
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(|id| (id.to_string(), answer))
            .collect()
    };
    [ids(yes, true), ids(no, false)].concat()
}

pub fn intake(app: &App, folder: &Path, options: &Options) -> Result<()> {
    let study = app.intake_study(folder, options.check, &Printed)?;
    let mut said = answers(options.yes, options.no);
    let mut plan = study.plan(&said);
    if options.ask {
        // Отложенные вопросы второй раз не задаём.
        let mut skipped: Vec<String> = Vec::new();
        while let Some(question) = plan.questions.iter().find(|q| !skipped.contains(&q.id)) {
            match ask(question, plan.questions.len() - skipped.len())? {
                Some(answer) => said.push((question.id.clone(), answer)),
                None => skipped.push(question.id.clone()),
            }
            plan = study.plan(&said);
        }
        println!();
    }
    print_plan(&plan, options.ask);
    if plan.items.is_empty() {
        return Ok(());
    }
    if !options.apply {
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

/// Задать вопрос в консоли. `None` — человек отложил ответ (Enter) или ввод
/// закончился.
fn ask(question: &Question, left: usize) -> Result<Option<bool>> {
    use std::io::Write;

    println!();
    println!("Вопрос (осталось {left}): {}", question.text);
    for fact in &question.facts {
        println!("    {fact}");
    }
    let hint = match question.suggested {
        Some(true) => " Похоже, что да.",
        Some(false) => " Похоже, что нет.",
        None => "",
    };
    loop {
        print!("  д — да, н — нет, Enter — решить потом.{hint} > ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            return Ok(None);
        }
        match line.trim().to_lowercase().chars().next() {
            None => return Ok(None),
            Some('д' | 'y' | 'l') => return Ok(Some(true)),
            Some('н' | 'n') => return Ok(Some(false)),
            Some(_) => println!("  Не понял ответ."),
        }
    }
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

/// Показать план. `asked` — вопросы уже задавали: остались отложенные.
fn print_plan(plan: &IntakePlan, asked: bool) {
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
    if !plan.questions.is_empty() {
        println!();
        if asked {
            println!("Отложенные вопросы (план выполнится и без ответа):");
        } else {
            println!("Вопросы — ответить по одному: добавьте к команде --ask");
        }
        for (n, question) in plan.questions.iter().enumerate() {
            println!("  {}. {}", n + 1, question.text);
            for fact in &question.facts {
                println!("       {fact}");
            }
        }
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
