//! Отрисовка изменений: `changes.png` с цветной подсветкой.

use std::path::{Path, PathBuf};

use nonius_core::compare::{ChangeReport, CompareContext};
use nonius_core::extension::{ExtensionPoint, Module, ModuleManifest, Renderer, Snapshot};
use nonius_core::{Error, Result};

use super::{manifest, model_pair, module_err, shared_diff, CHANGES_PNG};
use crate::render::View;
use crate::save_png;

/// Картинка изменений: `changes.png` с цветной подсветкой.
pub struct HighlightRenderer {
    manifest: ModuleManifest,
    view: View,
}

impl HighlightRenderer {
    /// Идентификатор модуля.
    pub const ID: &'static str = "highlight-occt";

    /// Создать модуль с видом по умолчанию.
    pub fn new() -> Self {
        Self {
            manifest: manifest(
                Self::ID,
                "Цветная подсветка изменений",
                ExtensionPoint::Renderer,
            ),
            view: View::default(),
        }
    }
}

impl Default for HighlightRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for HighlightRenderer {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl Renderer for HighlightRenderer {
    fn render(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        report: &ChangeReport,
        out_dir: &Path,
    ) -> Result<Vec<PathBuf>> {
        self.render_with(before, after, report, &CompareContext::default(), out_dir)
    }

    /// С допуском проекта: та же разница, что в отчёте сравнения, — и она
    /// не считается второй раз.
    fn render_with(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        _report: &ChangeReport,
        ctx: &CompareContext,
        out_dir: &Path,
    ) -> Result<Vec<PathBuf>> {
        let Some((a, b)) = model_pair(before, after) else {
            return Ok(Vec::new());
        };
        let diff = shared_diff(&a, &b, ctx.tolerance_mm).map_err(|e| module_err(Self::ID, e))?;
        let img = diff
            .render(&self.view)
            .map_err(|e| module_err(Self::ID, e))?;
        std::fs::create_dir_all(out_dir).map_err(|e| Error::Io {
            path: out_dir.to_path_buf(),
            source: e,
        })?;
        let path = out_dir.join(CHANGES_PNG);
        save_png(&img, &path).map_err(|e| Error::Io {
            path: path.clone(),
            source: e,
        })?;
        Ok(vec![path])
    }
}
