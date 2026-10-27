//! Экспорт `preview`: превью детали `preview.png` из `model.step` версии.

use std::path::{Path, PathBuf};

use nonius_core::extension::{Exporter, ExtensionPoint, Module, ModuleManifest, Snapshot};
use nonius_core::{Error, Result};

use super::{manifest, module_err, MODEL_STEP};

/// Формат экспорта «превью детали» (так его ищет программа).
const FORMAT: &str = "preview";

/// Имя превью, которое пишет [`PreviewExporter`].
pub const PREVIEW_PNG: &str = "preview.png";

/// Превью детали в PNG.
pub struct PreviewExporter {
    manifest: ModuleManifest,
}

impl PreviewExporter {
    /// Идентификатор модуля.
    pub const ID: &'static str = "preview-occt";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: manifest(Self::ID, "Превью детали", ExtensionPoint::Exporter),
        }
    }
}

impl Default for PreviewExporter {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for PreviewExporter {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl Exporter for PreviewExporter {
    fn formats(&self) -> Vec<String> {
        vec![FORMAT.into()]
    }

    fn export(&self, snapshot: &Snapshot, format: &str, out_dir: &Path) -> Result<PathBuf> {
        if !format.eq_ignore_ascii_case(FORMAT) {
            return Err(Error::Module {
                module: Self::ID.into(),
                message: format!("формат {format} не поддерживается"),
            });
        }
        let step = snapshot.dir.join(MODEL_STEP);
        std::fs::create_dir_all(out_dir).map_err(|e| Error::Io {
            path: out_dir.to_path_buf(),
            source: e,
        })?;
        let path = out_dir.join(PREVIEW_PNG);
        crate::preview_png(&step, &path).map_err(|e| module_err(Self::ID, e))?;
        Ok(path)
    }
}
