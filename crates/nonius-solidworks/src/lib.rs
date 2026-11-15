//! STEP из файла SolidWorks без ручной выгрузки: Nonius сам просит об этом
//! установленный SolidWorks.
//!
//! Зачем: `.sldprt` — закрытый формат, прочитать его напрямую нельзя. Раньше
//! человек при каждом импорте выгружал STEP руками, иначе форма, масса и
//! сравнение оставались от прошлой версии. Теперь импорт `.sldprt` сам
//! получает свежий STEP.
//!
//! Как: SolidWorks умеет выполнять команды других программ (это называется
//! COM, тот же механизм, что у макросов). Модуль держит один рабочий
//! процесс (скрипт PowerShell `worker.ps1`), а тот запускает **свой**,
//! невидимый экземпляр SolidWorks и просит его «сохранить как STEP». К
//! SolidWorks, открытому человеком, модуль не подключается и общих настроек
//! не меняет: там несохранённая работа, а трудная деталь может SolidWorks
//! уронить. Зависший или упавший свой SolidWorks перезапускается, деталь,
//! которая роняет его дважды, получает окончательный отказ.
//!
//! Модуль подключается к точке [`Converter`] и только там, где SolidWorks
//! установлен ([`available`]); без него импорт работает по-старому.
//!
//! Второй модуль — связи файлов (точка [`LinkSource`]): из каких деталей
//! собрана сборка и к какой модели относится чертёж. По ним Nonius сам
//! раскладывает папку с файлами.
//!
//! Для окна: [`status`] — что сейчас со своим SolidWorks (свободен,
//! работает над файлом, перезапускается, не установлен), [`own`] — его
//! номер процесса и время старта.
//!
//! Файлы: здесь — модуль `solidworks-step`; `worker.ps1` и `helper.cs` —
//! рабочий процесс и свой экземпляр SolidWorks; `wire.rs` — слова разговора
//! с ним; `worker.rs` — труба к процессу; `session.rs` — надзор за каждой
//! командой (сроки, «умер», «занят», перезапуск); `hub.rs` — один сеанс на
//! программу, очередь и состояние; `run.rs` — есть ли SolidWorks, выгрузка
//! STEP; `stamp.rs` — от чего зависит STEP (для склада); `links.rs` —
//! модуль `solidworks-links`.

mod hub;
mod links;
mod run;
mod session;
mod stamp;
mod wire;
mod worker;

use std::path::Path;

use nonius_core::extension::{
    ConvertAsk, Converter, ExtensionPoint, Module, ModuleManifest, Registry, API_VERSION,
};
use nonius_core::{Error, Result};

use hub::Ask;
pub use hub::{own, status, Status};
pub use links::SolidWorksLinks;
pub use run::{available, export_step, ping};
use session::Failed;
pub use wire::Own;

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
        self.to_step_asked(source, target, &ConvertAsk::plain())
    }

    fn to_step_asked(&self, source: &Path, target: &Path, ask: &ConvertAsk) -> Result<()> {
        let module = Self::ID.to_string();
        let ask = Ask {
            urgent: ask.urgent,
            place: ask.place,
            notify: ask.notify,
            cancelled: ask.cancelled,
        };
        run::step(source, target, &ask).map_err(|failed| match failed {
            Failed::Refused(message) => Error::Refused { module, message },
            Failed::Later(message) => Error::Module { module, message },
            Failed::Cancelled => Error::Module {
                module,
                message: "остановлено".into(),
            },
        })
    }

    fn stamp(&self, source: &Path) -> Option<String> {
        stamp::stamp(source)
    }
}

/// Подключить модули, если на этом компьютере есть SolidWorks.
pub fn register(registry: &mut Registry) -> Result<()> {
    if available() {
        registry.add_converter(Box::new(SolidWorksConverter::new()))?;
        registry.add_link_source(Box::new(SolidWorksLinks::new()))?;
    }
    Ok(())
}
