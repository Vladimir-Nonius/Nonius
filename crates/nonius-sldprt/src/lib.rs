//! Модули Nonius, которые читают файл SolidWorks напрямую, без SolidWorks.
//!
//! SolidWorks будят редко — ради точной формы и спорных деталей. Всё
//! остальное лежит в самом файле `.sldprt` / `.sldasm` / `.slddrw`:
//!
//! - **Форма** (`glance.rs`, `mesh.rs`, точка [`ShapeGlance`]). В файле
//!   сохранена сетка, которой SolidWorks рисует деталь на экране. По ней
//!   считаются объём, площадь, габариты и число граней — за миллисекунды.
//!   Числа приблизительные (сетка — многогранник): на настоящих деталях
//!   объём расходится с точным не больше чем на 1–2 %, габариты — до
//!   0,6 %, а граней меньше, чем в STEP. Поэтому это «беглый взгляд», а не
//!   паспорт версии: при импорте форму по-прежнему считают по STEP.
//! - **Замысел** (`design/`, `source.rs`, точка [`DesignSource`]): список
//!   операций с размерами, конфигурации, свойства (масса, материал, ЕСКД),
//!   состав сборки, модель чертежа, признаки «спорной» детали.
//! - **Картинка** — готовое превью из файла ([`preview_png`]).
//!
//! Модули не гадают. Беглый взгляд отказывает, когда файл старого формата
//! (контейнер OLE) или не SolidWorks, сетки в файле нет или она старее
//! модели; а когда числа сняты с оговорками (сетка не замкнута, деталь
//! производная от другой) — отдаёт их только как «прочитано частично»,
//! с перечнем оговорок. Замысел читается так же: что прочиталось — то и
//! отдано, а рядом список пропусков.
//!
//! Формат разобран по настоящим файлам; что известно о нём — в базе
//! знаний `docs/formats/`.
//!
//! Файлы: `container/` — секции файла, `xml.rs` — терпимое чтение XML,
//! `mesh.rs` — сетка и числа по ней, `glance.rs` — модуль формы,
//! `design/` — замысел, `source.rs` — модуль замысла.
//!
//! [`ShapeGlance`]: nonius_core::extension::ShapeGlance
//! [`DesignSource`]: nonius_core::extension::DesignSource

mod container;
pub mod design;
mod glance;
mod mesh;
mod source;
pub mod xml;

use nonius_core::extension::Registry;
use nonius_core::Result;

pub use container::{Budget, Container, Listing, Refusal, Section, Unreadable};
pub use design::{Design, Doubt, Gap};
pub use glance::{fingerprint, look, preview_png, SldprtGlance, GAP_CUT, GAP_DERIVED, GAP_OPEN};
pub use mesh::{measure, survey, Measured, Survey};
pub use source::{to_core, SldprtDesign};

/// Подключить модули: беглый взгляд на форму и замысел из файла.
/// SolidWorks им не нужен.
pub fn register(registry: &mut Registry) -> Result<()> {
    registry.add_glance(Box::new(SldprtGlance::new()))?;
    registry.add_design_source(Box::new(SldprtDesign::new()))
}
