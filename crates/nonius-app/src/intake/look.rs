//! «Показать в 3D»: две модели из разбираемой папки — сравнение формы,
//! как у версий детали (общее серым, убранное красным, добавленное
//! зелёным). Человек видит, что изменилось, и отвечает на вопрос разбора
//! уверенно.
//!
//! Деталей ещё нет в проекте, поэтому сравниваются файлы с диска
//! ([`App::compare_files`]). Для файла SolidWorks нужен STEP: свежий рядом
//! или от SolidWorks (`words.rs::step_of`) — это может идти минуту, зовите
//! из фоновой работы.

use std::path::{Path, PathBuf};

use crate::{App, Error, FilesCompared, Progress, Result};

impl App {
    /// Сравнить две модели разбираемой папки: `before` и `after` — файлы
    /// одной модели (`вал.sldprt` и `вал.step`), как в
    /// [`PlannedVersion::models`](super::PlannedVersion).
    pub fn intake_compare(
        &self,
        before: &[PathBuf],
        after: &[PathBuf],
        progress: &dyn Progress,
    ) -> Result<FilesCompared> {
        let step = |models: &[PathBuf]| {
            let main = models.last().map_or_else(PathBuf::new, Clone::clone);
            progress.step(&format!("Беру STEP: {}", name(&main)));
            let mut warnings = Vec::new();
            let got = self.step_of(models, &mut warnings);
            if progress.cancelled() {
                return Err(Error::Cancelled);
            }
            got.ok_or_else(|| Error::NoStep {
                path: main,
                why: if warnings.is_empty() {
                    "рядом нет файла STEP, а SolidWorks на этом компьютере не найден".into()
                } else {
                    warnings.join("; ")
                },
            })
        };
        let was = step(before)?;
        let now = step(after)?;
        let model = |s: &super::words::Step| s.dir.join("model.step");
        let mut done = self.compare_files(&model(&was), &model(&now), progress)?;
        done.step_behind = was.stale || now.stale;
        Ok(done)
    }
}

fn name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
