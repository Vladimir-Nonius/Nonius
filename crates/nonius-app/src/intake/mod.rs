//! Разбор папки: человек показывает папку, где модели, чертежи и бумаги
//! лежат вперемешку, а Nonius предлагает, как разложить её в проект.
//!
//! Два шага, и между ними — решение человека:
//! 1. [`App::intake_plan`] ничего не меняет. Он смотрит папку и возвращает
//!    [`IntakePlan`]: какие будут детали и сборки, какие файлы — версии
//!    одной детали, какой чертёж к какой детали, что осталось непонятым.
//!    У каждого решения записано «почему».
//! 2. [`App::intake_apply`] выполняет план: копирует файлы в проект и
//!    сохраняет версии. Исходная папка не меняется.
//!
//! План — обычные данные: его одинаково показывают окно, команда
//! `nonius intake` и ИИ, а править его может и человек, и программа.
//! Будущий аддин SolidWorks спросит то же самое про один файл.
//!
//! Откуда берутся догадки:
//! - одно имя с разными расширениями (`вал.sldprt`, `вал.step`, `вал.pdf`) —
//!   одна деталь;
//! - чертёж идёт к модели, на которую он ссылается (точка `LinkSource`,
//!   отвечает SolidWorks), даже если назван иначе;
//! - `вал`, `вал_v2`, `вал (3)` — версии одной детали (`names.rs`), но не
//!   тогда, когда оба файла стоят в одной сборке: это разные детали;
//! - догадку по именам проверяет форма (`shape.rs`): похожие — версии,
//!   непохожие — разные детали, одинаковые — возможно, копии;
//! - детали, которые нужны только одной сборке, кладутся в её папку.
//!
//! Файлы: здесь — данные плана и обход папки, `names.rs` — пометки версий в
//! именах, `grouping.rs` — какие модели одна деталь и как её назвать,
//! `shape.rs` — сравнение формы, `measure.rs` — откуда берётся форма
//! (паспорт STEP, для файла SolidWorks — через модуль перевода),
//! `plan.rs` — остальные правила и сам план, `apply.rs` — выполнение.

mod apply;
mod grouping;
mod measure;
mod names;
mod plan;
mod shape;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::meta::ItemKind;
use nonius_core::PartName;
use serde::Serialize;

use crate::{App, Error, NoProgress, Progress, Result};

pub use apply::{IntakeDone, IntakeFailure};
pub use shape::ShapeVerdict;

/// Насколько глубоко проверять догадки формой деталей. Проверка читает
/// модели (файлы SolidWorks — через сам SolidWorks, секунды на деталь).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShapeCheck {
    /// Не проверять: только имена и связи. Мгновенно.
    Off,
    /// Проверить версии, угаданные по именам.
    #[default]
    Names,
    /// Посмотреть все детали: найдёт и похожие с непохожими именами.
    All,
}

/// План раскладки папки. Ничего не сделано, пока его не выполнят.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntakePlan {
    /// Папка, которую разбирали.
    pub source: PathBuf,
    /// Будущие детали и сборки, по папкам и именам.
    pub items: Vec<PlannedItem>,
    /// Файлы, которым место не нашлось: останутся, где лежат.
    pub left: Vec<LeftFile>,
}

/// Будущая деталь или сборка.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlannedItem {
    /// Имя в проекте.
    pub name: String,
    /// Деталь или сборка.
    pub kind: ItemKind,
    /// Папка в проекте от корня через `/`; пусто — корень.
    pub folder: String,
    /// Версии от старой к новой. Обычно одна.
    pub versions: Vec<PlannedVersion>,
    /// Чертежи и бумаги: лягут в `docs/` детали.
    pub docs: Vec<PathBuf>,
    /// Почему решено именно так — словами, для человека.
    pub why: Vec<String>,
    /// Версии угаданы по именам файлов: стоит показать человеку.
    pub guessed: bool,
    /// Что о догадке сказала форма деталей; `None` — не проверялось.
    pub shape: Option<ShapeVerdict>,
}

/// Одна версия будущей детали.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlannedVersion {
    /// Имя исходного файла без расширения: попадёт в описание версии.
    pub label: String,
    /// Файлы модели этой версии (`.sldprt` и его `.step`, если есть оба).
    pub models: Vec<PathBuf>,
}

/// Файл, который разбор оставил в покое.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LeftFile {
    /// Файл.
    pub path: PathBuf,
    /// Почему.
    pub why: String,
}

/// Причина «файл не наш»: не модель, не чертёж и не бумага конструктора.
/// Таких в рабочей папке бывают сотни (расчёты, логи) — показывать их
/// стоит числом, а не списком.
pub const NOT_OURS: &str = "не модель и не чертёж";

/// Файл, найденный в папке.
#[derive(Debug, Clone)]
struct Found {
    path: PathBuf,
    /// Папка внутри разбираемой, через `/`; пусто — сама папка.
    dir: String,
    stem: String,
    /// Расширение строчными буквами.
    ext: String,
    modified: std::time::SystemTime,
}

/// Ключ файла для сравнения путей: в Windows регистр букв не важен.
fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

impl App {
    /// Посмотреть папку `folder` и предложить, как разложить её в проект.
    /// Ничего не меняет ни в папке, ни в проекте. Версии, угаданные по
    /// именам, проверяются формой.
    pub fn intake_plan(&self, folder: &Path) -> Result<IntakePlan> {
        self.intake_plan_with(folder, ShapeCheck::default(), &NoProgress)
    }

    /// То же с выбором глубины проверки формой и строкой хода: проверка
    /// может идти минуты. «Остановить» — план будет с тем, что успели.
    pub fn intake_plan_with(
        &self,
        folder: &Path,
        check: ShapeCheck,
        progress: &dyn Progress,
    ) -> Result<IntakePlan> {
        let source = std::path::absolute(folder).map_err(|e| Error::io(folder, e))?;
        if !source.is_dir() {
            return Err(Error::FileNotFound(source));
        }
        let root = self.project.root();
        if source.starts_with(root) || root.starts_with(&source) {
            return Err(Error::IntakeInsideProject(source));
        }
        let mut files = Vec::new();
        walk(&source, "", &mut files)?;
        files.sort_by(|a, b| a.path.cmp(&b.path));

        let links = self.links_of(&files);
        let taken: Vec<PartName> = self.project.items()?.into_iter().map(|i| i.name).collect();
        // Сначала план по именам и связям: из него видно, чью форму смотреть.
        let draft = plan::build(
            source.clone(),
            files.clone(),
            &links,
            &taken,
            &HashMap::new(),
        );
        let shapes = self.shapes_of(&draft, check, progress);
        if shapes.is_empty() {
            return Ok(draft);
        }
        Ok(plan::build(source, files, &links, &taken, &shapes))
    }

    /// Связи файлов от подключённых модулей. Нет модуля или он не ответил —
    /// разбор идёт по одним именам.
    fn links_of(&self, files: &[Found]) -> HashMap<String, Vec<String>> {
        let mut links = HashMap::new();
        for source in self.registry.link_sources() {
            let ask: Vec<PathBuf> = files
                .iter()
                .filter(|f| source.knows(&f.path))
                .map(|f| f.path.clone())
                .collect();
            let Ok(answers) = source.links(&ask) else {
                continue;
            };
            for (file, targets) in ask.iter().zip(answers) {
                links
                    .entry(key(file))
                    .or_insert_with(Vec::new)
                    .extend(targets.iter().map(|t| key(t)));
            }
        }
        links
    }
}

/// Обойти папку. Скрытые папки, временные файлы SolidWorks (`~$вал.sldprt`)
/// и чужие проекты Nonius не трогаем.
fn walk(dir: &Path, rel: &str, out: &mut Vec<Found>) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::io(dir, e))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.starts_with("~$") {
            continue;
        }
        let kind = entry.file_type().map_err(|e| Error::io(&path, e))?;
        if kind.is_dir() {
            if path.join(nonius_core::project::PROJECT_FILE).is_file() {
                continue;
            }
            let inner = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            walk(&path, &inner, out)?;
        } else if kind.is_file() {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            out.push(Found {
                stem: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                ext: path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default(),
                dir: rel.to_string(),
                modified,
                path,
            });
        }
    }
    Ok(())
}
