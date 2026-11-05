//! Настройки проекта: название и допуск сравнения в `project.yaml`. Они едут
//! вместе с проектом; изменение — одна запись истории проекта.

use nonius_core::project::{Project, PROJECT_FILE};

use crate::{App, Error, Result};

/// Самый большой допуск, мм: больше — сравнение перестаёт что-либо видеть.
pub const MAX_TOLERANCE_MM: f64 = 10.0;

/// Настройки проекта, которые человек меняет сам.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSettings {
    /// Название проекта.
    pub name: String,
    /// Допуск сравнения, мм: отличия меньше него не считаются изменением.
    pub tolerance_mm: f64,
}

impl App {
    /// Настройки открытого проекта.
    pub fn project_settings(&self) -> ProjectSettings {
        let c = self.project.config();
        ProjectSettings {
            name: c.name.clone(),
            tolerance_mm: c.compare.tolerance_mm,
        }
    }

    /// Записать настройки в `project.yaml` и в историю проекта. Возвращает
    /// `false`, если менять было нечего. Открытый `App` видит старые
    /// значения: проект после этого открывают заново.
    pub fn set_project_settings(&self, new: &ProjectSettings) -> Result<bool> {
        let name = new.name.trim();
        if name.is_empty() || name.contains(['\n', '\r']) {
            return Err(Error::BadSettings(
                "название проекта не может быть пустым".into(),
            ));
        }
        let tolerance = new.tolerance_mm;
        if !(tolerance > 0.0 && tolerance <= MAX_TOLERANCE_MM) {
            return Err(Error::BadSettings(format!(
                "допуск {tolerance} мм не подходит: нужно число больше 0 и не больше {MAX_TOLERANCE_MM} мм"
            )));
        }
        let old = self.project_settings();
        if old.name == name && old.tolerance_mm == tolerance {
            return Ok(false);
        }
        self.project.writable()?;
        let store = self.store()?;
        let mut project = Project::open(self.project.root())?;
        project.update_config(|c| {
            c.name = name.to_string();
            c.compare.tolerance_mm = tolerance;
        })?;
        let message = format!(
            "Настройки проекта: «{name}», допуск {tolerance} мм\n\nИзменено в настройках Nonius.\n"
        );
        store.commit_project_file(PROJECT_FILE, &message)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nonius_core::extension::Registry;

    #[test]
    fn bad_settings_change_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        crate::create_project(tmp.path(), Some("Станок")).unwrap();
        let app = App::open(tmp.path(), Registry::new()).unwrap();
        let bad = |name: &str, tolerance_mm| ProjectSettings {
            name: name.into(),
            tolerance_mm,
        };
        for s in [bad(" ", 0.01), bad("Станок", 0.0), bad("Станок", 50.0)] {
            let e = app.set_project_settings(&s).unwrap_err();
            assert_eq!(e.code(), "bad_settings");
        }
        assert!(!app.set_project_settings(&bad("Станок", 0.01)).unwrap());
        assert_eq!(app.project_settings(), bad("Станок", 0.01));
    }
}
