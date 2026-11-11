//! Модели и то, что к ним приложено: `вал.sldprt` и `вал.step` — одна
//! модель; чертёж и бумаги идут к ней по ссылке или по имени.
//!
//! Здесь нет диска: только список найденных файлов и связи между ними.

use std::collections::HashMap;
use std::path::PathBuf;

use super::{key, Found, LeftFile};

/// Файлы моделей: из них получаются детали и сборки.
pub(super) const MODELS: &[&str] = &["sldprt", "sldasm", "step", "stp"];

/// Чертежи и бумаги конструктора: если такой файл ни к чему не подошёл,
/// человеку стоит о нём знать. Прочее (расчёты, логи, картинки программ) —
/// просто не наше.
pub(super) const PAPERS: &[&str] = &[
    "slddrw", "dxf", "dwg", "pdf", "doc", "docx", "xls", "xlsx", "odt", "ods",
];

/// Модель одного имени в одной папке со всем, что к ней приложено.
pub(super) struct Unit {
    pub dir: String,
    pub stem: String,
    pub models: Vec<Found>,
    pub docs: Vec<PathBuf>,
    pub why: Vec<String>,
}

impl Unit {
    pub fn is_assembly(&self) -> bool {
        self.models.iter().any(|m| m.ext == "sldasm")
    }
    /// Имя для ответов человека: папка и имя файла строчными буквами.
    pub fn id(&self) -> String {
        if self.dir.is_empty() {
            self.stem.to_lowercase()
        } else {
            format!("{}/{}", self.dir, self.stem).to_lowercase()
        }
    }
    /// Отпечаток содержимого главного файла модели (файл SolidWorks, а нет
    /// его — STEP), если его сняли: у побайтных копий он одинаковый.
    pub fn content(&self) -> Option<&str> {
        let main = self.models.iter().find(|m| m.ext.starts_with("sld"));
        main.or(self.models.first())?.digest.as_deref()
    }
    pub fn newest(&self) -> std::time::SystemTime {
        self.models
            .iter()
            .map(|m| m.modified)
            .max()
            .unwrap_or(std::time::UNIX_EPOCH)
    }
}

pub(super) fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(super) type Index = HashMap<(String, String), usize>;

/// Модели по папке и имени: `вал.sldprt` и `вал.step` — одна модель.
pub(super) fn units_of(files: &[Found]) -> (Vec<Unit>, Index, HashMap<String, usize>) {
    let mut units: Vec<Unit> = Vec::new();
    let mut by_name = Index::new();
    let mut by_path = HashMap::new();
    for f in files.iter().filter(|f| MODELS.contains(&f.ext.as_str())) {
        let id = (f.dir.clone(), f.stem.to_lowercase());
        let i = *by_name.entry(id).or_insert_with(|| {
            units.push(Unit {
                dir: f.dir.clone(),
                stem: f.stem.clone(),
                models: Vec::new(),
                docs: Vec::new(),
                why: Vec::new(),
            });
            units.len() - 1
        });
        units[i].models.push(f.clone());
        by_path.insert(key(&f.path), i);
    }
    (units, by_name, by_path)
}

/// Приложить чертежи и бумаги к моделям: по ссылке, потом по имени.
pub(super) fn attach(
    files: &[Found],
    links: &HashMap<String, Vec<String>>,
    by_name: &Index,
    by_path: &HashMap<String, usize>,
    units: &mut [Unit],
    left: &mut Vec<LeftFile>,
) {
    for f in files.iter().filter(|f| !MODELS.contains(&f.ext.as_str())) {
        let linked = links
            .get(&key(&f.path))
            .into_iter()
            .flatten()
            .find_map(|target| by_path.get(target).copied());
        let named = by_name
            .get(&(f.dir.clone(), f.stem.to_lowercase()))
            .copied();
        match (linked, named) {
            (Some(u), _) => {
                if named != Some(u) {
                    units[u].why.push(format!(
                        "чертёж «{}» ссылается на эту модель",
                        file_name(&f.path)
                    ));
                }
                units[u].docs.push(f.path.clone());
            }
            (None, Some(u)) => units[u].docs.push(f.path.clone()),
            (None, None) => left.push(LeftFile {
                path: f.path.clone(),
                why: if f.ext == "slddrw" {
                    "чертёж: не нашёл его модель в этой папке".into()
                } else if PAPERS.contains(&f.ext.as_str()) {
                    "не понял, к какой детали относится".into()
                } else {
                    super::NOT_OURS.into()
                },
            }),
        }
    }
}

/// Файлы модели в порядке импорта: сначала STEP, потом файл SolidWorks —
/// свежий STEP из SolidWorks, если его удастся получить, главнее старого.
pub(super) fn ordered(models: &[Found]) -> Vec<PathBuf> {
    let mut sorted: Vec<&Found> = models.iter().collect();
    sorted.sort_by_key(|m| (m.ext.starts_with("sld"), m.path.clone()));
    sorted.into_iter().map(|m| m.path.clone()).collect()
}
