//! Сравнение для окна: сначала деталь (она готова за секунды), потом цвета.
//! OpenCascade читает STEP по одному, поэтому сравнения идут по очереди.

use std::path::PathBuf;
use std::sync::TryLockError;

use anyhow::{bail, Result};
use nonius_app::CompareOptions;
use nonius_gui::source::{Comparison, Other, Pick, Progress};
use nonius_scene::Scene;

use super::{in_words, version, AppSource};

/// Ход сравнения → строка состояния окна; «Отменить» в окне → отмена расчёта.
struct Steps<'a>(&'a dyn Progress);

impl nonius_app::Progress for Steps<'_> {
    fn step(&self, what: &str) {
        self.0.step(&format!("{what}…"));
    }

    fn cancelled(&self) -> bool {
        self.0.cancelled()
    }
}

/// Прочитать сцену из файлов отрисовщиков.
fn read_scene(files: &[PathBuf]) -> (Option<Scene>, Option<String>) {
    let glb = files
        .iter()
        .find(|p| p.file_name().is_some_and(|n| n == nonius_occt::CHANGES_GLB));
    let Some(glb) = glb else {
        return (None, None);
    };
    match std::fs::read(glb)
        .map_err(anyhow::Error::from)
        .and_then(|b| Ok(Scene::from_glb(&b)?))
    {
        Ok(s) => (Some(s), None),
        Err(e) => (None, Some(format!("{e:#}"))),
    }
}

impl AppSource {
    /// Деталь как она есть. Сцены нет — вторым значением идёт причина.
    fn model_scene(
        &self,
        part: &str,
        pick: Pick,
        steps: &Steps,
    ) -> (Option<Scene>, Option<String>) {
        let files = version(pick).and_then(|at| Ok(self.app.model_view(part, at, steps)?));
        match files {
            Ok(files) => read_scene(&files),
            Err(e) => match e.downcast_ref() {
                Some(nonius_app::Error::NoRenderer) => (None, None),
                _ => (None, Some(format!("{e:#}"))),
            },
        }
    }

    pub(super) fn compare_pair(
        &self,
        part: &str,
        before: Pick,
        after: Pick,
        progress: &dyn Progress,
    ) -> Result<Comparison> {
        let from = version(before)?;
        if from.is_none() && before != after {
            bail!("«Было» — это сохранённая версия: сравнивать можно с ней");
        }
        let _one_at_a_time = match self.busy.try_lock() {
            Ok(g) => g,
            Err(TryLockError::Poisoned(e)) => e.into_inner(),
            // Прошлое (отменённое) сравнение доделывает свой шаг.
            Err(TryLockError::WouldBlock) => {
                progress.step("Жду, пока закончится прошлое сравнение…");
                self.busy.lock().unwrap_or_else(|e| e.into_inner())
            }
        };
        let _files = self.reading();
        let steps = Steps(progress);
        // Сначала деталь: она готова за секунды, цвета считаются дольше.
        let (model, model_problem) = self.model_scene(part, after, &steps);
        let Some(from) = from.filter(|_| before != after) else {
            return Ok(Comparison {
                scene: model,
                scene_problem: model_problem,
                ..Comparison::default()
            });
        };
        if let Some(scene) = &model {
            progress.model(scene.clone());
        }
        let options = CompareOptions {
            reports: true,
            render: true,
        };
        let to = version(after)?;
        let compared = self.app.compare(part, Some(from), to, &options, &steps);
        // Тела разницы освобождаются, пока OpenCascade занят этой работой.
        nonius_occt::forget_diff();
        let c = match compared {
            Ok(c) => c,
            // Цвета не получились (нет OpenCascade, булева операция не
            // сошлась) — деталь и числа всё равно покажем, а причину скажем.
            Err(nonius_app::Error::Cancelled) => bail!("сравнение отменено"),
            Err(e) => {
                let problem = match e {
                    nonius_app::Error::NoRenderer => None,
                    e => Some(format!("{e:#}")),
                };
                let numbers = CompareOptions::default();
                let c = self.app.compare(part, Some(from), to, &numbers, &steps)?;
                return Ok(Comparison {
                    step_behind: c.step_behind(),
                    scene: model,
                    scene_problem: problem,
                    reports: c.reports,
                });
            }
        };
        let (scene, scene_problem) = read_scene(&c.files);
        Ok(Comparison {
            step_behind: c.step_behind(),
            scene: scene.or(model),
            scene_problem,
            reports: c.reports,
        })
    }

    pub(super) fn compare_with(
        &self,
        part: &str,
        pick: Pick,
        other: &Other,
        progress: &dyn Progress,
    ) -> Result<Comparison> {
        let _one_at_a_time = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        let _files = self.reading();
        let steps = Steps(progress);
        let (model, model_problem) = self.model_scene(part, pick, &steps);
        if let Some(scene) = &model {
            progress.model(scene.clone());
        }
        let other = match other {
            Other::File(path) => nonius_app::Other::File(path),
            Other::Part(name) => nonius_app::Other::Part(name),
        };
        let at = version(pick)?;
        let options = CompareOptions {
            reports: true,
            render: true,
        };
        let compared = self.app.compare_outside(part, at, other, &options, &steps);
        nonius_occt::forget_diff();
        match compared {
            Ok(done) => {
                let (scene, scene_problem) = read_scene(&done.files);
                Ok(Comparison {
                    scene: scene.or(model),
                    scene_problem,
                    reports: done.reports,
                    step_behind: false,
                })
            }
            Err(nonius_app::Error::Cancelled) => bail!("сравнение отменено"),
            Err(nonius_app::Error::NoRenderer) => {
                let numbers = CompareOptions::default();
                let done = self
                    .app
                    .compare_outside(part, at, other, &numbers, &steps)?;
                Ok(Comparison {
                    scene: model,
                    scene_problem: model_problem,
                    reports: done.reports,
                    step_behind: false,
                })
            }
            Err(e) => Err(in_words(e)),
        }
    }
}
