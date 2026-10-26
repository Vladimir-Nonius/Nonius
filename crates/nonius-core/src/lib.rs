//! Ядро Nonius — системы контроля версий для CAD-моделей.
//!
//! Ядро намеренно маленькое: формат папки проекта, файлы со схемой версии,
//! имена деталей и версий, точки расширения для модулей. Всё остальное
//! (чтение STEP, подсветка изменений, аддин SolidWorks, интерфейс) — модули,
//! которые подключаются через [`extension`].
//!
//! Архитектура описана в `docs/ARCHITECTURE.md`.

pub mod compare;
pub mod contract;
pub mod error;
pub mod extension;
pub mod meta;
pub mod names;
pub mod project;
pub mod schema;
pub mod tree;

pub use error::{Error, Result};
pub use meta::PartMeta;
pub use names::{PartName, VersionNumber};
pub use project::{Project, ProjectConfig};
