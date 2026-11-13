//! Что изменилось между двумя версиями — словами инженера: «отверстие
//! ⌀8 → ⌀10», «убрано скругление R3». Это главная улика в плане и в
//! вопросах: по числам («объём −4 %») человек свою деталь не узнает, а по
//! словам — вспомнит.
//!
//! Слова дают «быстрые» модули сравнения (`Comparator::quick`) — те, что
//! отвечают за доли секунды и не считают разницу объёмов. Им нужен STEP
//! обеих версий: готовый, если он не старше файла SolidWorks, иначе — от
//! модуля перевода. Посчитанное запоминается (`intake-words.tsv` рядом с
//! памятью о форме): второй разбор SolidWorks не ждёт.
//!
//! Правила плана (`plan.rs`) сами ничего не считают: они спрашивают у
//! [`Words`] готовое, а чего нет — то записывается в «недостающее», и
//! изучение папки (`study.rs`) досчитывает это один раз.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use nonius_core::compare::CompareContext;
use nonius_core::extension::Snapshot;
use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::PartName;

use super::key;
use super::measure::{is_step, modified};
use super::memory::sign;
use crate::{App, Progress};

/// Сколько изменений показывать: остальное — числом.
const SHOWN: usize = 5;
/// Имя файла памяти в рабочей папке программы.
const FILE: &str = "intake-words.tsv";
/// Разделитель строк одного ответа в файле памяти.
const BREAK: char = '\u{1f}';

/// Пара версий: файлы модели «было» и «стало».
pub(super) type Pair = (Vec<PathBuf>, Vec<PathBuf>);

/// Слова об изменениях для пар версий.
#[derive(Default)]
pub(super) struct Words {
    known: HashMap<(String, String), Vec<String>>,
    /// О чём спрашивали, а ответа нет.
    missed: Mutex<Vec<Pair>>,
}

fn pair_key(before: &[PathBuf], after: &[PathBuf]) -> Option<(String, String)> {
    Some((key(before.first()?), key(after.first()?)))
}

impl Words {
    pub fn new(known: HashMap<(String, String), Vec<String>>) -> Self {
        Self {
            known,
            missed: Mutex::default(),
        }
    }

    /// Что изменилось от `before` к `after`. Пусто — не знаем или ничего.
    pub fn get(&self, before: &[PathBuf], after: &[PathBuf]) -> Vec<String> {
        let Some(pair) = pair_key(before, after) else {
            return Vec::new();
        };
        if let Some(lines) = self.known.get(&pair) {
            return lines.clone();
        }
        if let Ok(mut missed) = self.missed.lock() {
            missed.push((before.to_vec(), after.to_vec()));
        }
        Vec::new()
    }

    /// Пары, о которых спрашивали, но слов для них нет.
    pub fn missed(&self) -> Vec<Pair> {
        let mut pairs = self.missed.lock().map(|m| m.clone()).unwrap_or_default();
        pairs.sort();
        pairs.dedup();
        pairs
    }
}

/// STEP одной версии в своей временной папке под именем `model.step` —
/// так его ищут модули сравнения. Папка живёт, пока жив этот объект.
pub(super) struct Step {
    pub dir: PathBuf,
    /// STEP старше файла SolidWorks, а свежий получить не вышло: форма
    /// может быть прежней.
    pub stale: bool,
    _keep: Box<dyn std::any::Any>,
}

impl App {
    /// Посчитать слова для пар версий. Чего не вышло — того просто нет.
    pub(super) fn words_of(
        &self,
        pairs: &[Pair],
        progress: &dyn Progress,
    ) -> HashMap<(String, String), Vec<String>> {
        let mut known = HashMap::new();
        if !self.registry.has_quick_comparator() {
            return known;
        }
        let file = crate::work_dir(self.project.root()).join(FILE);
        let mut memory = load(&file);
        let mut added = false;
        for (n, (before, after)) in pairs.iter().enumerate() {
            if progress.cancelled() {
                break;
            }
            let (Some(pair), Some(signs)) = (pair_key(before, after), signs(before, after)) else {
                continue;
            };
            let lines = match memory.get(&signs) {
                Some(lines) => lines.clone(),
                None => {
                    progress.step(&format!(
                        "Смотрю, что изменилось: {} из {}",
                        n + 1,
                        pairs.len()
                    ));
                    let Some(lines) = self.compare_words(before, after) else {
                        continue;
                    };
                    memory.insert(signs, lines.clone());
                    added = true;
                    lines
                }
            };
            known.insert(pair, lines);
        }
        if added {
            save(&file, &memory);
        }
        known
    }

    fn compare_words(&self, before: &[PathBuf], after: &[PathBuf]) -> Option<Vec<String>> {
        let mut warnings = Vec::new();
        let was = self.step_of(before, &mut warnings)?;
        let now = self.step_of(after, &mut warnings)?;
        let snapshot = |step: &Step| {
            Some(Snapshot {
                item: PartName::new("разбор").ok()?,
                version: None,
                dir: step.dir.clone(),
                meta: PartMeta::new(ItemKind::Part),
                tree: None,
            })
        };
        let ctx = CompareContext {
            tolerance_mm: self.project.config().compare.tolerance_mm,
            ..CompareContext::default()
        };
        let reports = self
            .registry
            .compare_quick(&snapshot(&was)?, &snapshot(&now)?, &ctx)
            .ok()?;
        let mut lines: Vec<String> = reports
            .iter()
            .flat_map(|r| r.changes.iter().map(ToString::to_string))
            .collect();
        if lines.len() > SHOWN {
            let rest = lines.len() - SHOWN;
            lines.truncate(SHOWN);
            lines.push(format!("и ещё изменений: {rest}"));
        }
        Some(lines)
    }

    /// STEP версии: готовый, если он не старше файла SolidWorks рядом,
    /// иначе — от модуля перевода (почему не вышло — в `warnings`).
    pub(super) fn step_of(&self, models: &[PathBuf], warnings: &mut Vec<String>) -> Option<Step> {
        let step = models.iter().find(|m| is_step(m));
        let cad = models.iter().find(|m| !is_step(m));
        let fresh = match (step, cad) {
            (Some(s), Some(c)) => modified(s) >= modified(c),
            (Some(_), None) => true,
            _ => false,
        };
        let copied = |file: &Path, stale| {
            let dir = tempfile::tempdir().ok()?;
            std::fs::copy(file, dir.path().join("model.step")).ok()?;
            Some(Step {
                dir: dir.path().to_path_buf(),
                stale,
                _keep: Box::new(dir),
            })
        };
        if fresh {
            return copied(step?, false);
        }
        let converted = cad.and_then(|c| self.convert(c, warnings));
        match converted {
            Some(c) => Some(Step {
                dir: c.step.parent()?.to_path_buf(),
                stale: false,
                _keep: Box::new(c),
            }),
            // SolidWorks не помог — лучше старый STEP, чем ничего.
            None => copied(step?, true),
        }
    }
}

fn signs(before: &[PathBuf], after: &[PathBuf]) -> Option<(String, String)> {
    Some((sign(before)?, sign(after)?))
}

fn load(file: &Path) -> HashMap<(String, String), Vec<String>> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let mut cells = line.split('\t');
            let pair = (cells.next()?.to_string(), cells.next()?.to_string());
            let lines = cells.next()?.split(BREAK).filter(|l| !l.is_empty());
            Some((pair, lines.map(String::from).collect()))
        })
        .collect()
}

/// Записать память. Не вышло — не беда: это кэш.
fn save(file: &Path, memory: &HashMap<(String, String), Vec<String>>) {
    let mut lines: Vec<String> = memory
        .iter()
        .map(|((a, b), words)| format!("{a}\t{b}\t{}\n", words.join(&BREAK.to_string())))
        .collect();
    lines.sort();
    // Только сама рабочая папка, не `.git` над ней (см. `memory.rs`).
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir(dir);
    }
    let _ = std::fs::write(file, lines.concat());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_pairs_are_remembered_as_missed() {
        let (a, b) = (
            vec![PathBuf::from("/п/вал.step")],
            vec![PathBuf::from("/п/Вал_v2.step")],
        );
        let words = Words::default();
        assert!(words.get(&a, &b).is_empty());
        assert!(words.get(&a, &b).is_empty());
        assert_eq!(words.missed(), [(a.clone(), b.clone())], "без повторов");

        let known = HashMap::from([(
            pair_key(&a, &b).unwrap(),
            vec!["отверстие ⌀8 → ⌀10".to_string()],
        )]);
        let words = Words::new(known);
        assert_eq!(words.get(&a, &b), ["отверстие ⌀8 → ⌀10"]);
        assert!(words.missed().is_empty());
    }

    #[test]
    fn memory_survives_a_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(FILE);
        let pair = ("было".to_string(), "стало".to_string());
        let memory = HashMap::from([
            (pair.clone(), vec!["раз".to_string(), "два".to_string()]),
            (("а".to_string(), "б".to_string()), Vec::new()),
        ]);
        save(&file, &memory);
        assert_eq!(load(&file), memory, "и «ничего не изменилось» — тоже ответ");
    }
}
