//! Откуда берётся форма деталей для проверки догадок (`shape.rs`).
//!
//! Форма — это объём, площадь, габариты, число граней. Два способа снять
//! её ([`Source`]):
//!
//! - беглый взгляд в сам файл CAD (точка `ShapeGlance`, модуль
//!   `nonius-sldprt` читает сетку показа из `.sldprt`): доли секунды,
//!   чужая программа не нужна, числа чуть грубее;
//! - точно, по STEP: готовый STEP рядом, если он не старше файла CAD, или
//!   STEP от модуля перевода (SolidWorks: секунды на деталь, а если он
//!   завис — минуты).
//!
//! Работа идёт в два прохода. Первый ([`Pass::Glance`]) чужую программу не
//! будит: у всех версий группы — беглая форма (у кого её нет — готовый
//! STEP), и план готов за секунды. Группы, где беглой форме нечего сказать
//! (файл без сетки, а STEP нет) или где ответ на грани (`shape::doubtful`),
//! — спорные. Второй проход ([`Pass::Exact`]) досчитывает у спорных групп
//! точную форму всех версий, теперь и через SolidWorks.
//!
//! У всех версий одной группы числа по возможности из одного источника:
//! так «копия ли это» видно точно. Смотрим не всё подряд, а только то, что
//! просили ([`ShapeCheck`]). Снятая форма запоминается (`memory.rs`):
//! второй разбор той же папки файлы заново не читает.
//!
//! Ничего не вышло (нет модуля, SolidWorks не ответил) — не беда: у детали
//! просто не будет отпечатка, и догадка останется непроверенной.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use nonius_core::meta::ItemKind;

use super::memory::Memory;
use super::shape::{doubtful, Shape, Source};
use super::{key, IntakePlan, ShapeCheck};
use crate::import::Converted;
use crate::{App, Progress};

pub(super) fn is_step(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| e == "step" || e == "stp")
}

pub(super) fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Проход снятия формы.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pass {
    /// Быстро, без чужих программ.
    Glance,
    /// Спорные группы — точно, хоть через SolidWorks.
    Exact,
}

/// Снятая форма: по ключу каждого файла модели (см. [`key`]).
pub(super) struct Shapes {
    pub by_file: HashMap<String, Shape>,
    /// Файлы моделей спорных групп, которые второй проход ещё уточнит.
    /// После второго прохода пусто.
    pub pending: HashSet<String>,
}

/// STEP от модуля перевода за один точный проход: форма и слова одной
/// версии просят его у SolidWorks один раз. Без этого склада (`None` там,
/// где его ждут) модуль перевода не зовётся вовсе.
#[derive(Default)]
pub(super) struct Exports(RefCell<HashMap<String, Option<Rc<Converted>>>>);

impl Exports {
    /// STEP файла CAD `cad`; не вышло — почему, в `warnings` (только в
    /// первый раз).
    pub fn of(&self, app: &App, cad: &Path, warnings: &mut Vec<String>) -> Option<Rc<Converted>> {
        if let Some(known) = self.0.borrow().get(&key(cad)) {
            return known.clone();
        }
        let got = app.convert(cad, warnings).map(Rc::new);
        self.0.borrow_mut().insert(key(cad), got.clone());
        got
    }
}

/// Версия: подпись для строки хода и файлы модели.
type Version<'a> = (&'a str, &'a [PathBuf]);

/// Группы версий, чью форму стоит смотреть: детали чернового плана.
fn wanted(draft: &IntakePlan, check: ShapeCheck) -> Vec<Vec<Version<'_>>> {
    // Сборки не смотрим: перевод большой сборки в STEP — это минуты.
    draft
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Part)
        .filter(|i| match check {
            ShapeCheck::Off => false,
            ShapeCheck::Names => i.guessed,
            ShapeCheck::All => true,
        })
        .map(|i| {
            let versions = i.versions.iter();
            versions
                .map(|v| (v.label.as_str(), v.models.as_slice()))
                .collect()
        })
        .collect()
}

/// Спорная группа: у версии нет формы (и неизвестно, что её не будет —
/// `hopeless`) или пара версий на грани ответа.
fn disputed(shapes: &[Option<Shape>], hopeless: &[bool]) -> bool {
    if shapes.len() < 2 {
        return false;
    }
    let known: Vec<&Shape> = shapes.iter().flatten().collect();
    let missing = shapes.iter().zip(hopeless).any(|(s, &h)| s.is_none() && !h);
    missing
        || known
            .iter()
            .enumerate()
            .any(|(n, a)| known[n + 1..].iter().any(|b| doubtful(a, b)))
}

impl App {
    /// Снять отпечатки формы у моделей чернового плана `draft`.
    pub(super) fn shapes_of(
        &self,
        draft: &IntakePlan,
        check: ShapeCheck,
        pass: Pass,
        progress: &dyn Progress,
        exports: &Exports,
    ) -> Shapes {
        let mut memory = Memory::load(&crate::work_dir(self.project.root()));
        let groups = wanted(draft, check);
        let total: usize = groups.iter().map(Vec::len).sum();
        let mut seen = 0;
        let mut found: Vec<Vec<Option<Shape>>> = Vec::new();
        // Версии, у которых точной формы не будет: SolidWorks уже пробовали.
        let hopeless: Vec<Vec<bool>> = groups
            .iter()
            .map(|g| g.iter().map(|(_, m)| memory.failed(m)).collect())
            .collect();
        for group in &groups {
            // Уже уточнённая раньше группа — сразу точная, если помнятся все.
            let exact: Vec<Option<Shape>> = group
                .iter()
                .map(|(_, m)| memory.get(m, Source::Exact))
                .collect();
            if exact.iter().all(Option::is_some) {
                seen += group.len();
                found.push(exact);
                continue;
            }
            let mut shapes = Vec::new();
            for (label, models) in group {
                seen += 1;
                if progress.cancelled() {
                    shapes.push(None);
                    continue;
                }
                // Беглой нет, а точная помнится (файл без сетки) — тоже знаем.
                let known = memory
                    .get(models, Source::Glance)
                    .or_else(|| memory.get(models, Source::Exact));
                if known.is_none() {
                    progress.step(&format!("Смотрю форму {seen} из {total}: {label}"));
                }
                let shape = known
                    .or_else(|| self.glance(models, &mut memory))
                    .or_else(|| self.exact(models, None, &mut memory));
                shapes.push(shape);
            }
            found.push(shapes);
        }
        let mut pending = HashSet::new();
        let disputes: Vec<usize> = (0..groups.len())
            .filter(|&g| disputed(&found[g], &hopeless[g]))
            .collect();
        let refined: usize = disputes.iter().map(|&g| groups[g].len()).sum();
        let mut done = 0;
        for &g in &disputes {
            if pass == Pass::Glance || progress.cancelled() {
                pending.extend(groups[g].iter().flat_map(|(_, m)| m.iter().map(|f| key(f))));
                continue;
            }
            for (v, (label, models)) in groups[g].iter().enumerate() {
                done += 1;
                progress.step(&format!("Уточняю форму {done} из {refined}: {label}"));
                if let Some(shape) = self.exact(models, Some(exports), &mut memory) {
                    found[g][v] = Some(shape);
                }
            }
        }
        memory.save();
        let mut by_file = HashMap::new();
        for (group, shapes) in groups.iter().zip(&found) {
            for ((label, models), shape) in group.iter().zip(shapes) {
                let Some(shape) = shape else { continue };
                if std::env::var_os("NONIUS_TRACE").is_some() {
                    eprintln!("форма «{label}»: {shape:?}");
                }
                for model in *models {
                    by_file.insert(key(model), *shape);
                }
            }
        }
        Shapes { by_file, pending }
    }

    /// Беглый взгляд в сам файл CAD. `None` — некому смотреть или в файле
    /// нет годных данных; причина видна с `NONIUS_TRACE`.
    fn glance(&self, models: &[PathBuf], memory: &mut Memory) -> Option<Shape> {
        let cad = models.iter().find(|m| !is_step(m))?;
        match self.registry.glance_for(cad)?.glance(cad) {
            Ok(summary) => {
                let shape = Shape::of(&summary, Source::Glance)?;
                memory.put(models, shape);
                Some(shape)
            }
            Err(e) => {
                if std::env::var_os("NONIUS_TRACE").is_some() {
                    eprintln!("беглый взгляд на {}: {e}", cad.display());
                }
                None
            }
        }
    }

    /// Точная форма одной версии: из памяти; готовый STEP, если он не
    /// старше файла CAD рядом; иначе (если есть склад `exports`) STEP
    /// просим у модуля перевода, а не вышло — лучше старый STEP, чем ничего.
    /// Если уже известно, что тела в STEP от модуля нет (`Memory::failed`),
    /// модуль не зовётся.
    fn exact(
        &self,
        models: &[PathBuf],
        exports: Option<&Exports>,
        memory: &mut Memory,
    ) -> Option<Shape> {
        if let Some(shape) = memory.get(models, Source::Exact) {
            return Some(shape);
        }
        let step = models.iter().find(|m| is_step(m));
        let cad = models.iter().find(|m| !is_step(m));
        let mut ignored = Vec::new();
        let read = |file: &Path, ignored: &mut Vec<String>| {
            let meta = self.read_geometry(file, ignored).ok().flatten()?;
            Shape::of(&meta.geometry, Source::Exact)
        };
        let fresh = match (step, cad) {
            (Some(s), Some(c)) => modified(s) >= modified(c),
            (Some(_), None) => true,
            _ => false,
        };
        let shape = if fresh {
            read(step?, &mut ignored)
        } else if let Some(exports) = exports.filter(|_| !memory.failed(models)) {
            let converted = cad.and_then(|c| exports.of(self, c, &mut ignored));
            let got = converted
                .as_ref()
                .and_then(|c| c.read.as_ref())
                .and_then(|m| Shape::of(&m.geometry, Source::Exact));
            // STEP получен, а тела в нём нет (поверхности, пустая деталь):
            // так будет и в следующий раз — запомнить и не просить снова.
            // Не ответил SolidWorks — может, ответит потом: не запоминаем.
            if converted.is_some() && got.is_none() {
                memory.fail(models);
            }
            got.or_else(|| read(step?, &mut ignored))
        } else {
            step.filter(|_| exports.is_some())
                .and_then(|s| read(s, &mut ignored))
        };
        if std::env::var_os("NONIUS_TRACE").is_some() {
            for why in &ignored {
                eprintln!("точная форма {}: {why}", models[0].display());
            }
        }
        let shape = shape?;
        memory.put(models, shape);
        Some(shape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(volume: f64, source: Source) -> Option<Shape> {
        Some(Shape {
            volume,
            area: None,
            dims: [20.0, 20.0, 100.0],
            faces: None,
            source,
        })
    }

    #[test]
    fn group_is_disputed_when_glance_cannot_tell() {
        use Source::{Exact, Glance};
        // Одна версия — сравнивать не с чем, спорить не о чем.
        let (no, two) = (&[false, false], &[false, false]);
        assert!(!disputed(&[None], &[false]));
        // Беглой формы у версии нет — спорная.
        assert!(disputed(&[shape(10_000.0, Glance), None], two));
        // …если только SolidWorks уже не сказал, что формы не будет.
        assert!(!disputed(&[shape(10_000.0, Glance), None], &[false, true]));
        // Объём ушёл на 24 % — у порога «похожи / разные»: беглой не верим,
        // точной — верим.
        assert!(disputed(
            &[shape(10_000.0, Glance), shape(13_100.0, Glance)],
            no
        ));
        assert!(!disputed(
            &[shape(10_000.0, Exact), shape(13_100.0, Exact)],
            no
        ));
        // Далеко от порогов — беглой достаточно.
        assert!(!disputed(
            &[shape(10_000.0, Glance), shape(10_500.0, Glance)],
            no
        ));
        assert!(!disputed(
            &[shape(10_000.0, Glance), shape(30_000.0, Glance)],
            no
        ));
    }
}
