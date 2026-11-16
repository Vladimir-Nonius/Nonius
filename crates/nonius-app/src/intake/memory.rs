//! Память о форме деталей между разборами: `.git/nonius/intake-shapes.tsv`.
//!
//! Снять форму с файла SolidWorks — секунды на деталь, а в папке их
//! десятки. Чтобы второй разбор той же папки (после ответов на вопросы,
//! перед `--apply`) не ждал SolidWorks снова, форма запоминается. Запись
//! узнаётся по файлам модели: путь, размер и время изменения; файл
//! поменяли — запись не подойдёт, и форму снимут заново.
//!
//! Форму, снятую бегло (по сетке из самого файла), помечает слово `mesh2`
//! в конце строки. У одной версии могут быть обе записи: беглая — с
//! первого разбора, точная — после уточнения через SolidWorks.
//!
//! Цифра в пометке — поколение беглого взгляда. Когда он начинает
//! отказывать там, где раньше отвечал (так стало с производными деталями:
//! в их сетке лишние тела), цифру повышают, и строки прежних поколений —
//! у первого пометка была просто `mesh` — перестают читаться: форму снимут
//! заново. Строка с незнакомой пометкой пропускается, а не принимается за
//! точную.
//!
//! Строка `failed`, подпись и причина — точной формы у версии нет:
//! SolidWorks выгрузил STEP, а тела в нём нет (пустая «Деталь1»), или
//! SolidWorks с деталью не справился (завис, упал два раза подряд, не
//! открыл файл — причину словами даёт склад STEP, `steps.rs`). Такую
//! версию не уточняют при каждом разборе заново, пока файл не изменится.
//! Причина видна с `NONIUS_TRACE=1`.
//!
//! Это кэш: его можно удалить, испорченная строка просто пропускается.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::key;
use super::shape::{Shape, Source};

/// Пометка в конце строки: форма снята бегло.
const ROUGH: &str = "mesh2";
/// Начало строки: точной формы у версии нет.
const FAILED: &str = "failed";

/// Запомненные формы по подписи файлов модели.
pub(super) struct Memory {
    file: PathBuf,
    shapes: HashMap<(String, Source), Shape>,
    /// Подписи версий, у которых точной формы нет, и почему.
    failed: HashMap<String, String>,
    changed: bool,
}

/// Подпись версии: её файлы с размером и временем изменения. `None` —
/// файл не читается.
pub(super) fn sign(models: &[PathBuf]) -> Option<String> {
    let mut parts = Vec::new();
    for model in models {
        let meta = std::fs::metadata(model).ok()?;
        let at = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
        parts.push(format!("{}*{}*{}", key(model), meta.len(), at.as_millis()));
    }
    parts.sort();
    Some(parts.join("|"))
}

fn parse(line: &str) -> Option<((String, Source), Shape)> {
    let mut cells = line.split('\t');
    let mut number = || cells.next()?.parse::<f64>().ok();
    let volume = number()?;
    let area = number();
    let dims = [number()?, number()?, number()?];
    let faces = cells.next()?.parse::<u32>().ok();
    let sign = cells.next()?.to_string();
    let source = match cells.next() {
        None => Source::Exact,
        Some(ROUGH) => Source::Glance,
        // Пометка другого поколения: этой строке верить нельзя.
        Some(_) => return None,
    };
    let shape = Shape {
        volume,
        area,
        dims,
        faces,
        source,
    };
    Some(((sign, source), shape))
}

impl Memory {
    /// Прочитать память из рабочей папки программы `dir`.
    pub fn load(dir: &Path) -> Self {
        let file = dir.join("intake-shapes.tsv");
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        // Причины в старых строках нет — это «тела нет».
        let failed = text
            .lines()
            .filter_map(|l| l.strip_prefix(FAILED)?.strip_prefix('\t'))
            .map(|l| match l.split_once('\t') {
                Some((sign, why)) => (sign.to_string(), why.to_string()),
                None => (l.to_string(), "тела нет".to_string()),
            })
            .collect();
        Self {
            file,
            shapes: text.lines().filter_map(parse).collect(),
            failed,
            changed: false,
        }
    }

    /// Запомненная форма версии, снятая этим способом.
    pub fn get(&self, models: &[PathBuf], source: Source) -> Option<Shape> {
        self.shapes.get(&(sign(models)?, source)).copied()
    }

    pub fn put(&mut self, models: &[PathBuf], shape: Shape) {
        if let Some(sign) = sign(models) {
            self.shapes.insert((sign, shape.source), shape);
            self.changed = true;
        }
    }

    /// Точной формы у версии нет и не будет, пока файл тот же: `why` —
    /// причина словами («тела нет», «SolidWorks дважды не справился»).
    pub fn fail(&mut self, models: &[PathBuf], why: &str) {
        if let Some(sign) = sign(models) {
            if std::env::var_os("NONIUS_TRACE").is_some() {
                eprintln!("точной формы не будет ({why}): {}", models[0].display());
            }
            let why = why.replace(['\t', '\n', '\r'], " ");
            self.changed |= self.failed.insert(sign, why.clone()) != Some(why);
        }
    }

    /// Уже известно, что точной формы у версии нет.
    pub fn failed(&self, models: &[PathBuf]) -> bool {
        sign(models).is_some_and(|s| self.failed.contains_key(&s))
    }

    /// Почему точной формы у версии нет.
    #[cfg(test)]
    pub fn why_failed(&self, models: &[PathBuf]) -> Option<&str> {
        self.failed.get(&sign(models)?).map(String::as_str)
    }

    /// Записать, если что-то добавилось. Не вышло — не беда: это кэш.
    pub fn save(&self) {
        if !self.changed {
            return;
        }
        let cell = |v: Option<String>| v.unwrap_or_else(|| "-".into());
        let mut lines: Vec<String> = self
            .shapes
            .iter()
            .map(|((sign, _), s)| {
                format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{sign}{}\n",
                    s.volume,
                    cell(s.area.map(|a| a.to_string())),
                    s.dims[0],
                    s.dims[1],
                    s.dims[2],
                    cell(s.faces.map(|f| f.to_string())),
                    if s.source == Source::Glance {
                        format!("\t{ROUGH}")
                    } else {
                        String::new()
                    },
                )
            })
            .chain(
                self.failed
                    .iter()
                    .map(|(sign, why)| format!("{FAILED}\t{sign}\t{why}\n")),
            )
            .collect();
        lines.sort();
        // Только сама рабочая папка, не `.git` над ней: историю версий
        // проект заводит сам, и пустая `.git` ему помешала бы.
        if let Some(dir) = self.file.parent() {
            let _ = std::fs::create_dir(dir);
        }
        let _ = std::fs::write(&self.file, lines.concat());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_is_remembered_until_the_file_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let model = tmp.path().join("вал.sldprt");
        std::fs::write(&model, "раз").unwrap();
        let models = [model.clone()];
        let shape = Shape {
            volume: 439_112.904_579_328_8,
            area: None,
            dims: [70.0, 84.04, 397.9],
            faces: Some(352),
            source: Source::Exact,
        };
        let mut memory = Memory::load(tmp.path());
        assert_eq!(memory.get(&models, Source::Exact), None);
        memory.put(&models, shape);
        memory.save();

        let memory = Memory::load(tmp.path());
        assert_eq!(
            memory.get(&models, Source::Exact),
            Some(shape),
            "числа — без потерь"
        );
        assert_eq!(memory.get(&models, Source::Glance), None);
        // Беглая форма помнит, что она беглая, и живёт рядом с точной.
        let rough = Shape {
            volume: 440_000.0,
            source: Source::Glance,
            ..shape
        };
        let mut memory = memory;
        memory.put(&models, rough);
        memory.save();
        let memory = Memory::load(tmp.path());
        assert_eq!(memory.get(&models, Source::Glance), Some(rough));
        assert_eq!(memory.get(&models, Source::Exact), Some(shape));

        // Неудача помнится, пока файл тот же.
        let mut memory = memory;
        memory.fail(&models, "SolidWorks дважды не справился");
        memory.save();
        let memory = Memory::load(tmp.path());
        assert!(memory.failed(&models));
        assert_eq!(
            memory.why_failed(&models),
            Some("SolidWorks дважды не справился")
        );
        // Строка прежнего вида, без причины, читается как «тела нет».
        let old = format!("failed\t{}\n", sign(&models).unwrap());
        let file = tmp.path().join("intake-shapes.tsv");
        let kept = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, &old).unwrap();
        assert_eq!(
            Memory::load(tmp.path()).why_failed(&models),
            Some("тела нет")
        );
        std::fs::write(&file, kept).unwrap();
        assert_eq!(
            memory.get(&models, Source::Exact),
            Some(shape),
            "форма цела"
        );

        // Файл изменился — старая запись не годится.
        std::fs::write(&model, "раз и два").unwrap();
        assert_eq!(memory.get(&models, Source::Exact), None);
        assert!(!memory.failed(&models));
        // Испорченная строка пропускается.
        std::fs::write(tmp.path().join("intake-shapes.tsv"), "мусор\n").unwrap();
        assert_eq!(Memory::load(tmp.path()).get(&models, Source::Exact), None);
    }

    /// Беглая форма прежнего поколения (пометка `mesh`) не читается ни как
    /// беглая, ни как точная.
    #[test]
    fn rough_shape_of_an_older_generation_is_forgotten() {
        let tmp = tempfile::tempdir().unwrap();
        let model = tmp.path().join("вал.sldprt");
        std::fs::write(&model, "раз").unwrap();
        let models = [model];
        let line = |mark: &str| format!("100\t-\t1\t2\t3\t-\t{}{mark}\n", sign(&models).unwrap());
        let file = tmp.path().join("intake-shapes.tsv");
        std::fs::write(&file, line("\tmesh")).unwrap();
        let memory = Memory::load(tmp.path());
        assert_eq!(memory.get(&models, Source::Glance), None);
        assert_eq!(memory.get(&models, Source::Exact), None);
        std::fs::write(&file, line("\tmesh2")).unwrap();
        assert!(Memory::load(tmp.path())
            .get(&models, Source::Glance)
            .is_some());
        std::fs::write(&file, line("")).unwrap();
        assert!(Memory::load(tmp.path())
            .get(&models, Source::Exact)
            .is_some());
    }
}
