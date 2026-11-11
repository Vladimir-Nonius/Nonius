//! STEP из файла SolidWorks без ручной выгрузки: Nonius сам просит об этом
//! установленный SolidWorks.
//!
//! Зачем: `.sldprt` — закрытый формат, прочитать его напрямую нельзя. Раньше
//! человек при каждом импорте выгружал STEP руками, иначе форма, масса и
//! сравнение оставались от прошлой версии. Теперь импорт `.sldprt` сам
//! получает свежий STEP.
//!
//! Как: SolidWorks умеет выполнять команды других программ (это называется
//! COM, тот же механизм, что у макросов). Модуль запускает маленький скрипт
//! PowerShell (`export_step.ps1`), тот подключается к SolidWorks — к уже
//! открытому или запускает невидимый — и просит «сохранить как STEP».
//! Правила вежливости к человеку, который в это время работает в
//! SolidWorks, записаны в начале скрипта.
//!
//! Модуль подключается к точке [`Converter`] и только там, где SolidWorks
//! установлен ([`available`]); без него импорт работает по-старому.
//!
//! Файлы: здесь — модуль `solidworks-step`, `run.rs` — запуск скрипта и
//! разбор его ответа, `export_step.ps1` — сам разговор с SolidWorks.

mod run;

use std::path::Path;

use nonius_core::extension::{
    Converter, ExtensionPoint, Module, ModuleManifest, Registry, API_VERSION,
};
use nonius_core::{Error, Result};

pub use run::{available, export_step};

/// Модуль: STEP из `.sldprt` и `.sldasm` силами установленного SolidWorks.
pub struct SolidWorksConverter {
    manifest: ModuleManifest,
}

impl SolidWorksConverter {
    /// Идентификатор модуля.
    pub const ID: &'static str = "solidworks-step";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: ModuleManifest {
                id: Self::ID.into(),
                name: "STEP из SolidWorks".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                api: API_VERSION,
                points: vec![ExtensionPoint::Converter],
                permissions: vec![],
            },
        }
    }
}

impl Default for SolidWorksConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for SolidWorksConverter {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl Converter for SolidWorksConverter {
    fn can_convert(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("sldprt") || e.eq_ignore_ascii_case("sldasm"))
    }

    fn to_step(&self, source: &Path, target: &Path) -> Result<()> {
        export_step(source, target).map_err(|message| Error::Module {
            module: Self::ID.into(),
            message,
        })
    }
}

/// Подключить модуль, если на этом компьютере есть SolidWorks.
pub fn register(registry: &mut Registry) -> Result<()> {
    if available() {
        registry.add_converter(Box::new(SolidWorksConverter::new()))?;
    }
    Ok(())
}
