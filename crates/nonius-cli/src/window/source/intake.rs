//! Разбор папки для окна: изучение (долгое, один раз), план с ответами
//! человека (мгновенно) и раскладка. Правила те же, что у команды
//! `nonius intake`, — из `nonius-app`; здесь только перевод в то, что
//! показывает окно.

use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Result};
use nonius_app::{IntakeStudy, ShapeCheck, NOT_OURS};
use nonius_core::meta::ItemKind;
use nonius_gui::source::{
    IntakeItem, IntakeOutcome, IntakePlan, IntakeQuestion, IntakeSide, Progress,
};

use super::{in_words, AppSource};
use crate::commands::intake::unsure;

/// Ход разбора → строка окна; «Остановить» в окне → остановка разбора.
struct Steps<'a>(&'a dyn Progress);

impl nonius_app::Progress for Steps<'_> {
    fn step(&self, what: &str) {
        self.0.step(what);
    }

    fn cancelled(&self) -> bool {
        self.0.cancelled()
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Версии ядра → стороны сравнения в 3D. Сборки не сравниваются: пусто.
fn sides(versions: &[nonius_app::PlannedVersion]) -> Vec<IntakeSide> {
    let assembly = |v: &nonius_app::PlannedVersion| {
        v.models.iter().any(|m| {
            m.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("sldasm"))
        })
    };
    if versions.len() < 2 || versions.iter().any(assembly) {
        return Vec::new();
    }
    versions
        .iter()
        .map(|v| IntakeSide {
            label: v.label.clone(),
            models: v.models.clone(),
        })
        .collect()
}

/// План ядра → план для окна.
fn entry(plan: nonius_app::IntakePlan) -> IntakePlan {
    let items = plan
        .items
        .iter()
        .map(|item| IntakeItem {
            name: item.name.clone(),
            assembly: item.kind == ItemKind::Assembly,
            folder: item.folder.clone(),
            versions: item.versions.len(),
            why: item.why.clone(),
            docs: item.docs.iter().map(|d| file_name(d)).collect(),
            unsure: unsure(item),
            sides: if item.kind == ItemKind::Assembly {
                Vec::new()
            } else {
                sides(&item.versions)
            },
            changed: item.changed.clone(),
        })
        .collect();
    let questions = plan
        .questions
        .into_iter()
        .map(|q| IntakeQuestion {
            id: q.id,
            text: q.text,
            facts: q.facts,
            suggested: q.suggested,
            sides: sides(&q.versions),
            changed: q.changed,
        })
        .collect();
    let (junk, papers): (Vec<_>, Vec<_>) = plan.left.iter().partition(|l| l.why == NOT_OURS);
    IntakePlan {
        source: plan.source,
        items,
        questions,
        left: papers
            .into_iter()
            .map(|l| (file_name(&l.path), l.why.clone()))
            .collect(),
        other: junk.len(),
    }
}

impl AppSource {
    /// Изученная папка, если её уже изучали.
    fn studied(&self) -> Result<Arc<IntakeStudy>> {
        let study = self.study.lock().unwrap_or_else(|e| e.into_inner());
        match study.as_ref() {
            Some(study) => Ok(study.clone()),
            None => bail!("папка ещё не изучена: выберите её заново"),
        }
    }

    pub(super) fn study_folder(
        &self,
        folder: &Path,
        progress: &dyn Progress,
    ) -> Result<IntakePlan> {
        // Форму деталей читает OpenCascade: по одному чтению за раз.
        let _one_at_a_time = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        let study = self
            .app
            .intake_study(folder, ShapeCheck::Names, &Steps(progress))
            .map_err(in_words)?;
        let plan = entry(study.plan(&[]));
        *self.study.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(study));
        Ok(plan)
    }

    pub(super) fn plan_folder(&self, answers: &[(String, bool)]) -> Result<IntakePlan> {
        Ok(entry(self.studied()?.plan(answers)))
    }

    pub(super) fn apply_folder(
        &self,
        answers: &[(String, bool)],
        progress: &dyn Progress,
    ) -> Result<IntakeOutcome> {
        let plan = self.studied()?.plan(answers);
        let _one_at_a_time = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        let _files = self.writing();
        let done = self
            .app
            .intake_apply(&plan, &Steps(progress))
            .map_err(in_words)?;
        // Разложенное в проекте уже есть: второй раз тот же план не пройдёт.
        *self.study.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(IntakeOutcome {
            parts: done.saved.len(),
            versions: done.saved.iter().map(|(_, n)| n).sum(),
            warnings: done.warnings,
            failed: done.failed.into_iter().map(|f| (f.name, f.why)).collect(),
            stopped: done.stopped,
        })
    }
}

#[cfg(test)]
#[path = "intake_tests.rs"]
mod tests;
