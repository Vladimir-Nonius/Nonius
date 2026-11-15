//! Беглый взгляд на форму детали: модуль `sldprt-glance` (точка
//! [`ShapeGlance`]) и картинка-превью из файла.
//!
//! Исходов три:
//! - **прочитано** — сетка показа свежая и замкнутая: объём, площадь,
//!   габариты, число граней ([`fingerprint`], `ShapeGlance::glance`);
//! - **прочитано частично** — числа сняты, но с оговорками, и каждая
//!   записана пропуском (`ShapeGlance::glance_partial`, [`look`]):
//!   сетка не замкнута — объёма нет, а габариты и площадь верны; деталь
//!   производная от другой — в сетке могут быть лишние тела; файл обрезан;
//! - **отказ** — снять нечего: файл не открылся, сетки нет или она старее
//!   модели (метка в `…/DLUpdateStamp` не равна метке в `ModelStamps`).
//!
//! Строгий ответ отказывает и там, где частичный отвечает с оговорками:
//! числам строгого ответа верят без проверки.

use std::path::Path;

use nonius_core::design::{Gap, Partial};
use nonius_core::extension::{ExtensionPoint, Module, ModuleManifest, ShapeGlance, API_VERSION};
use nonius_core::meta::{BoundingBox, GeometrySummary};
use nonius_core::{Error, Result};

use crate::container::{Container, Listing};
use crate::design::Manifest;
use crate::mesh::{survey, Measured, Survey};
use crate::xml;

const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Код пропуска: сетка не замкнута, объёма нет.
pub const GAP_OPEN: &str = "mesh.open";
/// Код пропуска: деталь производная, в сетке могут быть лишние тела.
pub const GAP_DERIVED: &str = "mesh.derived";
/// Код пропуска: файл обрезан, каталога секций нет.
pub const GAP_CUT: &str = "catalog.missing";

/// Метка сетки (первые 4 байта `…/DLUpdateStamp`) и метка модели (байты
/// 4–8 `ModelStamps`) совпадают — сетка снята с того, что сейчас в файле.
fn mesh_is_fresh(file: &Container) -> bool {
    let mesh = file.chunk_by(|n| n.ends_with("/DLUpdateStamp"));
    let model = file.chunk("ModelStamps");
    match (mesh, model) {
        (Some(mesh), Some(model)) => {
            mesh.len() >= 4 && model.len() >= 8 && mesh[..4] == model[4..8]
        }
        _ => false,
    }
}

/// Деталь — производная от другой: в манифесте есть операция `Stock`.
/// На таких сетка показа расходилась с тем, что выгружает SolidWorks.
fn is_derived(file: &Container) -> bool {
    file.chunk("swXmlContents/Features")
        .and_then(xml::parse)
        .and_then(|doc| Manifest::read(&doc.root))
        .is_some_and(|m| m.external.iter().any(|e| e.kind == "Stock"))
}

/// Всё, что удалось снять с файла, и оговорки. Ошибка — словами, почему
/// снять нечего.
pub fn look(path: &Path) -> std::result::Result<Partial<Survey>, String> {
    let file = Container::open(path).map_err(|e| e.to_string())?;
    let Some(lists) = file.chunk("Contents/DisplayLists") else {
        return Err("в файле нет сетки показа (сохранён без неё)".into());
    };
    if !mesh_is_fresh(&file) {
        return Err("сетка показа в файле старее самой модели".into());
    }
    let Some(found) = survey(lists) else {
        return Err("сетка показа пуста".into());
    };
    let mut gaps = Vec::new();
    if file.listing() == Listing::Scan {
        let note = "файл обрезан: каталога секций нет, но сетка показа цела";
        gaps.push(Gap::new(GAP_CUT, note));
    }
    if !found.closed {
        let note = "сетка показа не замкнута: объёма нет, габариты и площадь есть";
        gaps.push(Gap::new(GAP_OPEN, note));
    }
    if is_derived(&file) {
        let note = "деталь сделана из другой детали: в сетке показа могут быть лишние тела";
        gaps.push(Gap::new(GAP_DERIVED, note));
    }
    Ok(Partial { value: found, gaps })
}

/// Числа формы из файла `.sldprt` — строгий ответ. Ошибка — словами,
/// почему числам верить нельзя.
pub fn fingerprint(path: &Path) -> std::result::Result<Measured, String> {
    let seen = look(path)?;
    // Обрезанный файл строгому ответу не мешает: сетка прошла проверку
    // контрольной суммой.
    match seen.gaps.iter().find(|g| g.code != GAP_CUT) {
        Some(gap) => Err(gap.note.clone()),
        None => Ok(seen.value.shape),
    }
}

/// Картинка-превью, которую SolidWorks сохранил в файле (PNG). `None` —
/// картинки нет.
pub fn preview_png(path: &Path) -> Option<Vec<u8>> {
    let file = Container::open(path).ok()?;
    let png = file.chunk("PreviewPNG")?;
    png.starts_with(&PNG).then(|| png.to_vec())
}

fn summary(shape: &Measured, with_volume: bool) -> GeometrySummary {
    GeometrySummary {
        volume_mm3: with_volume.then_some(shape.volume),
        area_mm2: Some(shape.area),
        bbox: Some(BoundingBox {
            min: shape.min,
            max: shape.max,
        }),
        faces: Some(shape.faces),
        ..GeometrySummary::default()
    }
}

/// Модуль: беглый взгляд на форму файла `.sldprt`.
pub struct SldprtGlance {
    manifest: ModuleManifest,
}

impl SldprtGlance {
    /// Идентификатор модуля.
    pub const ID: &'static str = "sldprt-glance";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: ModuleManifest {
                id: Self::ID.into(),
                name: "Форма из файла SolidWorks".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                api: API_VERSION,
                points: vec![ExtensionPoint::Glance],
                permissions: vec![],
            },
        }
    }

    fn refusal(message: String) -> Error {
        Error::Module {
            module: Self::ID.into(),
            message,
        }
    }
}

impl Default for SldprtGlance {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for SldprtGlance {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl ShapeGlance for SldprtGlance {
    fn can_glance(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("sldprt"))
    }

    fn glance(&self, path: &Path) -> Result<GeometrySummary> {
        let shape = fingerprint(path).map_err(Self::refusal)?;
        Ok(summary(&shape, true))
    }

    fn glance_partial(&self, path: &Path) -> Result<Partial<GeometrySummary>> {
        let seen = look(path).map_err(Self::refusal)?;
        Ok(Partial {
            value: summary(&seen.value.shape, seen.value.closed),
            gaps: seen.gaps,
        })
    }
}
