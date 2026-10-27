//! Импорт файла модели: положить файл в деталь и обновить паспорт.

use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::project::{Item, META_FILE};
use nonius_core::{schema, PartName};

use crate::materials::find_material;
use crate::passport::update_mass;
use crate::{App, Error, Result};

/// Формат экспорта «превью детали»: модуль, который его объявляет, рисует
/// `preview.png` для списка деталей.
pub const PREVIEW_FORMAT: &str = "preview";

/// Итог импорта.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// Деталь (создаётся, если её не было).
    pub item: Item,
    /// Имя файла модели в папке детали: `model.step`, `model.sldprt`, …
    pub target: &'static str,
    /// Паспорт после импорта.
    pub meta: PartMeta,
    /// Превью, если его удалось нарисовать.
    pub preview: Option<PathBuf>,
    /// Предупреждения: импорт прошёл, но не всё получилось.
    pub warnings: Vec<String>,
}

impl App {
    /// Добавить файл модели в деталь `part` (по умолчанию — имя файла без
    /// расширения; деталь создаётся, если её нет) и обновить паспорт.
    /// Материал и плотность сохраняются, масса пересчитывается по новому объёму.
    pub fn import(&self, file: &Path, part: Option<&str>) -> Result<Imported> {
        if !file.is_file() {
            return Err(Error::FileNotFound(file.to_path_buf()));
        }
        let ext = file
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let (target, kind) = match ext.as_str() {
            "step" | "stp" => ("model.step", ItemKind::Part),
            "sldprt" => ("model.sldprt", ItemKind::Part),
            "sldasm" => ("model.sldasm", ItemKind::Assembly),
            _ => return Err(Error::UnknownFormat(file.to_path_buf())),
        };
        let name = match part {
            Some(p) => p.to_string(),
            None => file
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };
        let name = PartName::new(name.as_str()).map_err(Error::BadItemName)?;

        let item = match self.project.find_item(&name)? {
            Some(i) => i,
            None => self.project.create_item(kind, &name)?,
        };
        let dest = item.dir.join(target);
        fs::copy(file, &dest).map_err(|e| Error::io(&dest, e))?;

        let mut warnings = Vec::new();
        let mut meta = self.project.load_meta(&item)?;
        if let Some(read) = self.read_geometry(file, &mut warnings)? {
            meta.geometry = read.geometry;
            meta.source = read.source;
            if read.material.is_some() && read.material != meta.material {
                // Новый материал из CAD: плотность — из справочника, если он там есть.
                meta.density_kg_m3 = read
                    .material
                    .as_deref()
                    .and_then(find_material)
                    .map(|m| m.density_kg_m3);
                meta.material = read.material;
            }
            meta.properties.extend(read.properties);
        } else if meta.source.cad.is_none() {
            meta.source.cad = Some("SolidWorks".into());
        }
        update_mass(&mut meta);
        schema::save(&meta, &item.dir.join(META_FILE))?;

        let preview = if target == "model.step" {
            self.make_preview(&item, &mut warnings)
        } else {
            None
        };
        Ok(Imported {
            item,
            target,
            meta,
            preview,
            warnings,
        })
    }

    /// Прочитать паспорт подходящим модулем. Если первый модуль не справился
    /// (пустой или повреждённый STEP), пробуем следующий и предупреждаем.
    fn read_geometry(&self, file: &Path, warnings: &mut Vec<String>) -> Result<Option<PartMeta>> {
        let mut last_err = None;
        for src in self.registry.geometry_sources_for(file) {
            match src.read(file) {
                Ok(m) => return Ok(Some(m)),
                Err(e) => {
                    warnings.push(format!("геометрия не прочитана: {e}"));
                    last_err = Some(e);
                }
            }
        }
        match last_err {
            Some(e) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Превью детали — модулем экспорта в формат [`PREVIEW_FORMAT`], если
    /// такой подключён. Модуль кладёт файл в папку детали.
    fn make_preview(&self, item: &Item, warnings: &mut Vec<String>) -> Option<PathBuf> {
        let exporter = self.registry.exporter_for(PREVIEW_FORMAT)?;
        let made = self
            .project
            .working_snapshot(item)
            .map_err(Error::from)
            .and_then(|s| Ok(exporter.export(&s, PREVIEW_FORMAT, &item.dir)?));
        match made {
            Ok(p) => Some(p),
            Err(e) => {
                warnings.push(format!("превью не сделано: {e}"));
                None
            }
        }
    }
}
