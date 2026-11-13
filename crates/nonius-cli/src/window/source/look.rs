//! «Показать в 3D» на экране разбора папки: две модели с диска — сцена
//! сравнения, как у версий детали. Не вышло посчитать разницу — две модели
//! рядом и причина.

use std::path::PathBuf;
use std::sync::TryLockError;

use anyhow::{bail, Result};
use nonius_gui::source::{Comparison, Progress};
use nonius_scene::Scene;

use super::compare::read_scene;
use super::{in_words, AppSource};

/// Ход работы → строка экрана разбора; «Остановить» → отмена.
struct Steps<'a>(&'a dyn Progress);

impl nonius_app::Progress for Steps<'_> {
    fn step(&self, what: &str) {
        self.0.step(&format!("{what}…"));
    }

    fn cancelled(&self) -> bool {
        self.0.cancelled()
    }
}

impl AppSource {
    pub(super) fn look_pair(
        &self,
        before: &[PathBuf],
        after: &[PathBuf],
        progress: &dyn Progress,
    ) -> Result<Comparison> {
        // OpenCascade читает STEP по одному: ждём прошлую работу.
        let _one_at_a_time = match self.busy.try_lock() {
            Ok(g) => g,
            Err(TryLockError::Poisoned(e)) => e.into_inner(),
            Err(TryLockError::WouldBlock) => {
                progress.step("Жду, пока закончится прошлая работа…");
                self.busy.lock().unwrap_or_else(|e| e.into_inner())
            }
        };
        let steps = Steps(progress);
        let compared = self.app.intake_compare(before, after, &steps);
        // Тела разницы освобождаются, пока OpenCascade занят этой работой.
        nonius_occt::forget_diff();
        let done = match compared {
            Ok(done) => done,
            Err(nonius_app::Error::Cancelled) => bail!("сравнение остановлено"),
            Err(e) => return Err(in_words(e)),
        };
        let (scene, mut problem) = read_scene(&done.files);
        let scene = match (&done.apart, scene) {
            (_, Some(scene)) => Some(scene),
            (Some((was, now)), None) => match (read_scene(was), read_scene(now)) {
                ((Some(a), _), (Some(b), _)) => Some(Scene::beside(&a, &b)),
                ((_, why), (_, other)) => {
                    problem = why.or(other).or(problem);
                    None
                }
            },
            (None, None) => None,
        };
        Ok(Comparison {
            scene,
            scene_problem: done.problem.clone().or(problem),
            reports: done.reports,
            step_behind: done.step_behind,
        })
    }
}
