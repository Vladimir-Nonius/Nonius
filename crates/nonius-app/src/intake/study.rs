//! Изучение папки: всё долгое (обход диска, связи от SolidWorks, форма
//! деталей) делается один раз и складывается в [`IntakeStudy`]. План из
//! него получается мгновенно и сколько угодно раз — после каждого ответа
//! человека на вопрос.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::PartName;

use super::questions::Decisions;
use super::shape::Shape;
use super::units::MODELS;
use super::{key, plan, Found, IntakePlan, ShapeCheck};
use crate::{App, Error, Progress, Result};

/// Изученная папка. Сама ничего не меняет.
pub struct IntakeStudy {
    source: PathBuf,
    files: Vec<Found>,
    links: HashMap<String, Vec<String>>,
    taken: Vec<PartName>,
    shapes: HashMap<String, Shape>,
}

impl IntakeStudy {
    /// План с учётом ответов человека: `id` вопроса и «да»/«нет». Без
    /// ответов — то, что программа решила сама, и её вопросы.
    pub fn plan(&self, answers: &[(String, bool)]) -> IntakePlan {
        plan::build(
            self.source.clone(),
            self.files.clone(),
            &self.links,
            &self.taken,
            &self.shapes,
            &Decisions::new(answers),
            false,
        )
    }
}

impl App {
    /// Изучить папку `folder`: файлы, связи между ними, форма деталей.
    /// Ничего не меняет ни в папке, ни в проекте. `check` — насколько
    /// глубоко смотреть форму (это минуты); «Остановить» — изучение
    /// закончится с тем, что успели.
    pub fn intake_study(
        &self,
        folder: &Path,
        check: ShapeCheck,
        progress: &dyn Progress,
    ) -> Result<IntakeStudy> {
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
        digests(&mut files);

        let mut study = IntakeStudy {
            links: self.links_of(&files),
            taken: self.project.items()?.into_iter().map(|i| i.name).collect(),
            shapes: HashMap::new(),
            source,
            files,
        };
        // Сначала черновик по именам и связям: из него видно, чью форму
        // смотреть. В нём одно имя в разных папках считается одной деталью.
        let draft = plan::build(
            study.source.clone(),
            study.files.clone(),
            &study.links,
            &study.taken,
            &study.shapes,
            &Decisions::new(&[]),
            true,
        );
        study.shapes = self.shapes_of(&draft, check, progress);
        Ok(study)
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

/// Снять отпечатки содержимого у моделей, которые могут быть побайтными
/// копиями: то же расширение и тот же размер. Остальные файлы не читаем.
fn digests(files: &mut [Found]) {
    use sha2::{Digest, Sha256};

    let mut sizes: HashMap<(String, u64), Vec<usize>> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        if !MODELS.contains(&f.ext.as_str()) {
            continue;
        }
        if let Ok(meta) = fs::metadata(&f.path) {
            sizes
                .entry((f.ext.clone(), meta.len()))
                .or_default()
                .push(i);
        }
    }
    for i in sizes.into_values().filter(|v| v.len() > 1).flatten() {
        let Ok(mut file) = fs::File::open(&files[i].path) else {
            continue;
        };
        let mut hasher = Sha256::new();
        if std::io::copy(&mut file, &mut hasher).is_ok() {
            let hex: String = hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            files[i].digest = Some(hex);
        }
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
                digest: None,
                path,
            });
        }
    }
    Ok(())
}
