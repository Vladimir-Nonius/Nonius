//! Точки расширения: как модули подключаются к ядру.
//!
//! Принцип — маленькое стабильное ядро, всё остальное модули. Встроенные
//! функции (импорт STEP, подсветка изменений) подключаются через эти же
//! трейты, что и сторонние модули, поэтому интерфейсы проверяются с первого дня.
//!
//! Правила, чтобы модули ничего не ломали:
//! - у каждого модуля [`ModuleManifest`]: имя, версия, API, точки, права;
//! - модуль подключается только к точкам, объявленным в манифесте;
//! - API модулей версионирован ([`API_VERSION`]);
//! - для каждой точки есть контрактные тесты в [`crate::contract`].
//!
//! Точки: источник геометрии, источник дерева, сравнение, отрисовка,
//! экспорт, перевод файла CAD в STEP ([`Converter`]), связи файлов
//! ([`LinkSource`]), беглый взгляд на форму ([`ShapeGlance`]), события.
//!
//! Файлы: `manifest.rs` — манифест и права, `traits.rs` — трейты точек и
//! [`Snapshot`], `registry.rs` — реестр подключённых модулей.

mod manifest;
mod registry;
mod traits;

pub use manifest::{
    ExtensionPoint, Module, ModuleManifest, Permission, API_VERSION, MIN_API_VERSION,
};
pub use registry::Registry;
pub use traits::{
    Comparator, Converter, Event, EventHandler, Exporter, GeometrySource, LinkSource, Renderer,
    ShapeGlance, Snapshot, TreeSource,
};
