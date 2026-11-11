//! Выполнение плана разбора: копии файлов в проект и версии в историю.
//!
//! Исходная папка не меняется. Каждая деталь делается отдельно: не вышла
//! одна — остальные раскладываются, а про неё сказано в итоге.

use std::fs;
use std::path::Path;

use serde::Serialize;

use super::{IntakePlan, PlannedItem};
use crate::{App, Progress, Result};

/// Что получилось.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct IntakeDone {
    /// Разложенные детали: имя и сколько версий сохранено.
    pub saved: Vec<(String, u32)>,
    /// Детали, которые разложить не удалось.
    pub failed: Vec<IntakeFailure>,
    /// Замечания: всё сделано, но что-то не так (STEP не получен, бумага с
    /// таким именем уже лежит).
    pub warnings: Vec<String>,
    /// Человек остановил разбор: остальное не тронуто.
    pub stopped: bool,
}

/// Деталь, которую разложить не удалось.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntakeFailure {
    /// Имя детали из плана.
    pub name: String,
    /// Что случилось.
    pub why: String,
    /// Сколько её версий успело сохраниться.
    pub saved: u32,
}

impl App {
    /// Выполнить план: разложить файлы и сохранить версии. `progress`
    /// получает строку хода на каждую деталь; «Остановить» действует между
    /// деталями.
    pub fn intake_apply(&self, plan: &IntakePlan, progress: &dyn Progress) -> Result<IntakeDone> {
        self.project.writable()?;
        let mut done = IntakeDone::default();
        let total = plan.items.len();
        for (n, item) in plan.items.iter().enumerate() {
            if progress.cancelled() {
                done.stopped = true;
                break;
            }
            progress.step(&format!("Раскладываю {} из {total}: {}", n + 1, item.name));
            let mut saved = 0;
            match self.intake_item(item, &mut saved, &mut done.warnings) {
                Ok(()) => done.saved.push((item.name.clone(), saved)),
                Err(e) => done.failed.push(IntakeFailure {
                    name: item.name.clone(),
                    why: e.to_string(),
                    saved,
                }),
            }
        }
        Ok(done)
    }

    fn intake_item(
        &self,
        plan: &PlannedItem,
        saved: &mut u32,
        warnings: &mut Vec<String>,
    ) -> Result<()> {
        let folder = Path::new(&plan.folder);
        if !plan.folder.is_empty() && !self.project.root().join(folder).is_dir() {
            self.new_folder(folder)?;
        }
        let item = self.new_item_in(&plan.name, plan.kind, folder)?;
        let filled = (|| -> Result<()> {
            for (n, version) in plan.versions.iter().enumerate() {
                for model in &version.models {
                    let imported = self.import(model, Some(&plan.name))?;
                    warnings.extend(
                        imported
                            .warnings
                            .iter()
                            .map(|w| format!("{}: {w}", plan.name)),
                    );
                }
                if n == 0 {
                    for doc in &plan.docs {
                        if let Err(e) = self.add_file(&plan.name, doc) {
                            warnings.push(format!("{}: {e}", plan.name));
                        }
                    }
                }
                let message = if plan.versions.len() > 1 {
                    format!(
                        "Разбор папки: «{}», версия {} из {}",
                        version.label,
                        n + 1,
                        plan.versions.len()
                    )
                } else {
                    format!("Разбор папки: «{}»", version.label)
                };
                self.save_version(&plan.name, &message)?;
                *saved += 1;
            }
            Ok(())
        })();
        // Ничего не сохранилось — не оставляем в проекте пустую деталь.
        if filled.is_err() && *saved == 0 {
            let _ = fs::remove_dir_all(&item.dir);
        }
        filled
    }
}
