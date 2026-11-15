//! Изучение папки: всё долгое (обход диска, связи от SolidWorks, форма
//! деталей, что изменилось между версиями) делается один раз и складывается
//! в [`IntakeStudy`]. План из него получается мгновенно и сколько угодно
//! раз — после каждого ответа человека на вопрос.
//!
//! Изучение идёт в два прохода (см. `measure.rs`):
//! 1. [`App::intake_glance`] — быстро, чужих программ не будит: форма
//!    бегло из самих файлов, связей файлов ещё нет. План — за секунды.
//! 2. [`App::intake_refine`] — точно: связи от SolidWorks, точная форма
//!    спорных деталей, слова «что изменилось» для всех пар. Окно делает
//!    это в фоне, пока человек смотрит первый план; команда — сразу
//!    ([`App::intake_study`]), если её не попросили «только быстро».

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use nonius_core::PartName;

use super::disk::{digests, walk};
use super::exports::Exports;
use super::measure::Pass;
use super::plan::Facts;
use super::questions::Decisions;
use super::shape::Shape;
use super::words::Words;
use super::{key, plan, Found, IntakePlan, PlannedVersion, ShapeCheck};
use crate::{App, Error, Progress, Result};

/// Изученная папка. Сама ничего не меняет.
pub struct IntakeStudy {
    source: PathBuf,
    files: Vec<Found>,
    check: ShapeCheck,
    links: HashMap<String, Vec<String>>,
    taken: Vec<PartName>,
    shapes: HashMap<String, Shape>,
    /// Файлы моделей, чью форму уточнит второй проход.
    pending: HashSet<String>,
    words: Words,
    /// Второй, точный проход ещё может что-то добавить.
    unrefined: bool,
}

impl IntakeStudy {
    /// План с учётом ответов человека: `id` вопроса и «да»/«нет». Без
    /// ответов — то, что программа решила сама, и её вопросы.
    pub fn plan(&self, answers: &[(String, bool)]) -> IntakePlan {
        let facts = Facts {
            links: &self.links,
            taken: &self.taken,
            shapes: &self.shapes,
            said: &Decisions::new(answers),
            across: false,
            words: &self.words,
        };
        let mut plan = plan::build(self.source.clone(), self.files.clone(), facts);
        let pending = |versions: &[PlannedVersion]| {
            let mut models = versions.iter().flat_map(|v| v.models.iter());
            models.any(|m| self.pending.contains(&key(m)))
        };
        for item in &mut plan.items {
            item.doubtful = pending(&item.versions);
        }
        for question in &mut plan.questions {
            question.doubtful = pending(&question.versions);
        }
        plan
    }

    /// Есть ли смысл во втором, точном проходе ([`App::intake_refine`]):
    /// связи файлов ещё не спрошены, у спорных деталей форма беглая или у
    /// каких-то версий нет слов «что изменилось», — и есть модуль, который
    /// это даст (SolidWorks).
    pub fn needs_refine(&self) -> bool {
        self.unrefined
    }
}

impl App {
    /// Изучить папку `folder` целиком: оба прохода сразу. Ничего не меняет
    /// ни в папке, ни в проекте. `check` — насколько глубоко смотреть форму
    /// (это минуты); «Остановить» — изучение закончится с тем, что успели.
    pub fn intake_study(
        &self,
        folder: &Path,
        check: ShapeCheck,
        progress: &dyn Progress,
    ) -> Result<IntakeStudy> {
        let study = self.intake_glance(folder, check, progress)?;
        if study.needs_refine() && !progress.cancelled() {
            return Ok(self.intake_refine(&study, progress));
        }
        Ok(study)
    }

    /// Первый, быстрый проход: чужих программ не будит (связей файлов в нём
    /// нет, форма — бегло из самих файлов или из готового STEP). Спорные
    /// детали помечены в плане (`doubtful`); уточнит их
    /// [`intake_refine`](Self::intake_refine).
    pub fn intake_glance(
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
        let taken = self.project.items()?.into_iter().map(|i| i.name).collect();
        Ok(self.learn(source, files, taken, check, Pass::Glance, progress))
    }

    /// Второй, точный проход по уже изученной папке: связи файлов, точная
    /// форма спорных деталей и слова — через модули, которые будят чужую
    /// программу (SolidWorks). Минуты; «Остановить» — с тем, что успели.
    /// Прежнее изучение не меняется: ответ — новое.
    pub fn intake_refine(&self, study: &IntakeStudy, progress: &dyn Progress) -> IntakeStudy {
        let (source, files) = (study.source.clone(), study.files.clone());
        let taken = study.taken.clone();
        self.learn(source, files, taken, study.check, Pass::Exact, progress)
    }

    /// Один проход изучения по готовому списку файлов.
    fn learn(
        &self,
        source: PathBuf,
        files: Vec<Found>,
        taken: Vec<PartName>,
        check: ShapeCheck,
        pass: Pass,
        progress: &dyn Progress,
    ) -> IntakeStudy {
        let wake = pass == Pass::Exact;
        let store = Exports::background(progress);
        let exports = wake.then_some(&store);
        let mut study = IntakeStudy {
            links: if wake {
                self.links_of(&files, progress)
            } else {
                HashMap::new()
            },
            taken,
            check,
            shapes: HashMap::new(),
            pending: HashSet::new(),
            words: Words::default(),
            unrefined: false,
            source,
            files,
        };
        // Сначала черновик по именам и связям: из него видно, чью форму
        // смотреть. В нём одно имя в разных папках считается одной деталью.
        let facts = Facts {
            links: &study.links,
            taken: &study.taken,
            shapes: &study.shapes,
            said: &Decisions::new(&[]),
            across: true,
            words: &Words::default(),
        };
        let draft = plan::build(study.source.clone(), study.files.clone(), facts);
        let shapes = self.shapes_of(&draft, check, pass, progress, &store);
        study.shapes = shapes.by_file;
        study.pending = shapes.pending;
        // Теперь план с формой: из него видно, для каких пар версий стоит
        // узнать, что изменилось. Слова считаются один раз.
        let mut wordless = false;
        if check != ShapeCheck::Off {
            study.plan(&[]);
            let pairs = study.words.missed();
            let known = self.words_of(&pairs, exports, progress);
            wordless = known.len() < pairs.len();
            study.words = Words::new(known);
        }
        if !wake {
            let files = &study.files;
            let links = self
                .registry
                .link_sources()
                .any(|s| files.iter().any(|f| s.knows(&f.path)));
            let converts = files
                .iter()
                .any(|f| self.registry.converter_for(&f.path).is_some());
            // Уточнить нечем — спорное так и останется: не обещать.
            if !converts {
                study.pending.clear();
            }
            study.unrefined = links || !study.pending.is_empty() || converts && wordless;
        }
        study
    }

    /// Связи файлов от подключённых модулей. Нет модуля или он не ответил —
    /// разбор идёт по одним именам.
    fn links_of(&self, files: &[Found], progress: &dyn Progress) -> HashMap<String, Vec<String>> {
        let mut links = HashMap::new();
        for source in self.registry.link_sources() {
            let ask: Vec<PathBuf> = files
                .iter()
                .filter(|f| source.knows(&f.path))
                .map(|f| f.path.clone())
                .collect();
            let mut answers = None;
            progress.outside(&mut || answers = Some(source.links(&ask)));
            let Some(Ok(answers)) = answers else {
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
