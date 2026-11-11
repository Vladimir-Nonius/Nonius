//! Память о форме деталей между разборами: `.git/nonius/intake-shapes.tsv`.
//!
//! Снять форму с файла SolidWorks — секунды на деталь, а в папке их
//! десятки. Чтобы второй разбор той же папки (после ответов на вопросы,
//! перед `--apply`) не ждал SolidWorks снова, форма запоминается. Запись
//! узнаётся по файлам модели: путь, размер и время изменения; файл
//! поменяли — запись не подойдёт, и форму снимут заново.
//!
//! Это кэш: его можно удалить, испорченная строка просто пропускается.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::key;
use super::shape::Shape;

/// Запомненные формы по подписи файлов модели.
pub(super) struct Memory {
    file: PathBuf,
    shapes: HashMap<String, Shape>,
    changed: bool,
}

/// Подпись версии: её файлы с размером и временем изменения. `None` —
/// файл не читается.
fn sign(models: &[PathBuf]) -> Option<String> {
    let mut parts = Vec::new();
    for model in models {
        let meta = std::fs::metadata(model).ok()?;
        let at = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
        parts.push(format!("{}*{}*{}", key(model), meta.len(), at.as_millis()));
    }
    parts.sort();
    Some(parts.join("|"))
}

fn parse(line: &str) -> Option<(String, Shape)> {
    let mut cells = line.split('\t');
    let mut number = || cells.next()?.parse::<f64>().ok();
    let volume = number()?;
    let area = number();
    let dims = [number()?, number()?, number()?];
    let faces = cells.next()?.parse::<u32>().ok();
    let shape = Shape {
        volume,
        area,
        dims,
        faces,
    };
    Some((cells.next()?.to_string(), shape))
}

impl Memory {
    /// Прочитать память из рабочей папки программы `dir`.
    pub fn load(dir: &Path) -> Self {
        let file = dir.join("intake-shapes.tsv");
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        Self {
            file,
            shapes: text.lines().filter_map(parse).collect(),
            changed: false,
        }
    }

    pub fn get(&self, models: &[PathBuf]) -> Option<Shape> {
        self.shapes.get(&sign(models)?).copied()
    }

    pub fn put(&mut self, models: &[PathBuf], shape: Shape) {
        if let Some(sign) = sign(models) {
            self.shapes.insert(sign, shape);
            self.changed = true;
        }
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
            .map(|(sign, s)| {
                format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{sign}\n",
                    s.volume,
                    cell(s.area.map(|a| a.to_string())),
                    s.dims[0],
                    s.dims[1],
                    s.dims[2],
                    cell(s.faces.map(|f| f.to_string())),
                )
            })
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
        };
        let mut memory = Memory::load(tmp.path());
        assert_eq!(memory.get(&models), None);
        memory.put(&models, shape);
        memory.save();

        let memory = Memory::load(tmp.path());
        assert_eq!(memory.get(&models), Some(shape), "числа — без потерь");

        // Файл изменился — старая запись не годится.
        std::fs::write(&model, "раз и два").unwrap();
        assert_eq!(memory.get(&models), None);
        // Испорченная строка пропускается.
        std::fs::write(tmp.path().join("intake-shapes.tsv"), "мусор\n").unwrap();
        assert_eq!(Memory::load(tmp.path()).get(&models), None);
    }
}
