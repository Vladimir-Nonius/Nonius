//! Показ детали без сравнения: что нарисовать, пока у неё одна версия или
//! пока считаются цвета. Сравнение версии с собой обходится без булевых
//! операций, поэтому занимает секунды, и результат запоминается.

use std::path::PathBuf;

use nonius_core::compare::ChangeReport;
use nonius_core::extension::ExtensionPoint;
use nonius_core::schema::Versioned;
use nonius_core::VersionNumber;

use crate::cache::{self, CacheEntry};
use crate::compare::Progress;
use crate::{App, Error, Result};

impl App {
    /// Файлы отрисовщиков для детали как она есть: в версии `at` или сейчас.
    pub fn model_view(
        &self,
        name: &str,
        at: Option<VersionNumber>,
        progress: &dyn Progress,
    ) -> Result<Vec<PathBuf>> {
        if self.registry.renderers().next().is_none() {
            return Err(Error::NoRenderer);
        }
        let item = self.item(name)?;
        let version = at
            .map(|n| self.store()?.version(&item, n).map_err(Error::from))
            .transpose()?;
        let side = match &version {
            Some(v) => format!("версия {}", v.commit),
            None => format!("текущее {}", cache::model_fingerprint(&item.dir, false)?),
        };
        let label = at.map_or_else(|| "текущее".to_string(), |v| v.to_string());
        let base = CacheEntry {
            schema: CacheEntry::CURRENT,
            item: item.name.to_string(),
            from: label.clone(),
            to: label,
            modules: Vec::new(),
            reports: Vec::new(),
            files: Vec::new(),
        };
        let key = [format!("nonius-cache {}", CacheEntry::CURRENT), side];
        let modules = self.modules_of(ExtensionPoint::Renderer);
        let (entry, dir, _) = self.cached("model", &key, modules, &base, |dir| {
            let snapshot = match &version {
                Some(v) => self.store()?.snapshot(&item, v)?,
                None => self.project.working_snapshot(&item)?,
            };
            let report = ChangeReport::empty("nonius-app");
            let files = self.render_all(&snapshot, &snapshot, &report, dir, progress)?;
            Ok((Vec::new(), files))
        })?;
        Ok(entry.files.iter().map(|f| dir.join(f)).collect())
    }
}
