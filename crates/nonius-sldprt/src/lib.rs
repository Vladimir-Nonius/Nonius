//! Модуль Nonius: форма детали прямо из файла `.sldprt`, без SolidWorks.
//!
//! Разбор папки сверяет формой десятки деталей подряд. Просить STEP у
//! SolidWorks — секунды на деталь, а зависший SolidWorks — минуты. Но в
//! самом файле уже лежит сетка, которой SolidWorks рисует деталь на экране.
//! По ней считаются объём, площадь, габариты и число граней — за доли
//! секунды и без чужой программы.
//!
//! Числа приблизительные (сетка — многогранник): на настоящих деталях
//! объём расходится с точным не больше чем на 1–2 %, габариты — до 0,6 %,
//! а граней меньше, чем в STEP (там круглые грани разрезаны пополам).
//! Поэтому это «беглый взгляд» (точка [`ShapeGlance`]), а не паспорт
//! версии: при импорте форму по-прежнему считают по STEP.
//!
//! Модуль отказывается, а не гадает, когда:
//! - файл старого формата (до SolidWorks 2015) или не SolidWorks;
//! - сетки в файле нет (так сохраняют «без данных показа»);
//! - сетка старее модели — файл изменили, а сетку не обновили: метка в
//!   куске `…/DLUpdateStamp` не равна метке модели в `ModelStamps`;
//! - сетка не замкнута (поверхности, а не тело).
//!
//! После отказа форму узнают обычным путём — через STEP.
//!
//! Файлы: `container.rs` — куски файла, `mesh.rs` — сетка и числа по ней.
//! Ещё модуль отдаёт картинку-превью, сохранённую в файле
//! ([`preview_png`]).

mod container;
mod mesh;

use std::path::Path;

use nonius_core::extension::{
    ExtensionPoint, Module, ModuleManifest, Registry, ShapeGlance, API_VERSION,
};
use nonius_core::meta::{BoundingBox, GeometrySummary};
use nonius_core::{Error, Result};

pub use container::Container;
pub use mesh::{measure, Measured};

const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

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

/// Числа формы из файла `.sldprt`. Ошибка — словами, почему не вышло.
pub fn fingerprint(path: &Path) -> std::result::Result<Measured, String> {
    let file = Container::open(path).map_err(|e| format!("файл не читается: {e}"))?;
    let Some(lists) = file.chunk("Contents/DisplayLists") else {
        return Err("в файле нет сетки показа (старый формат или сохранён без неё)".into());
    };
    if !mesh_is_fresh(&file) {
        return Err("сетка показа в файле старее самой модели".into());
    }
    measure(&lists).ok_or_else(|| "сетка показа пуста или не замкнута".into())
}

/// Картинка-превью, которую SolidWorks сохранил в файле (PNG). `None` —
/// картинки нет.
pub fn preview_png(path: &Path) -> Option<Vec<u8>> {
    let png = Container::open(path).ok()?.chunk("PreviewPNG")?;
    png.starts_with(&PNG).then_some(png)
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
        let shape = fingerprint(path).map_err(|message| Error::Module {
            module: Self::ID.into(),
            message,
        })?;
        Ok(GeometrySummary {
            volume_mm3: Some(shape.volume),
            area_mm2: Some(shape.area),
            bbox: Some(BoundingBox {
                min: shape.min,
                max: shape.max,
            }),
            faces: Some(shape.faces),
            ..GeometrySummary::default()
        })
    }
}

/// Подключить модуль. SolidWorks ему не нужен.
pub fn register(registry: &mut Registry) -> Result<()> {
    registry.add_glance(Box::new(SldprtGlance::new()))
}
