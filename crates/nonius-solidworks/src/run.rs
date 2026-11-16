//! Просьбы к SolidWorks: есть ли он и выгрузка STEP.
//!
//! STEP выгружает **свой** экземпляр SolidWorks: его запускает рабочий
//! процесс (`worker.ps1`, труба к нему — `worker.rs`, слова — `wire.rs`),
//! один на всю программу (`hub.rs`: очередь и состояние; `session.rs`:
//! сроки, перезапуск). К SolidWorks, открытому человеком, Nonius не
//! подключается.
use std::path::Path;

use crate::hub::Ask;
use crate::session::Failed;
use crate::wire::{Command, Own};

/// Установлен ли SolidWorks: записан ли он в реестре Windows как программа,
/// которой можно давать команды. Не будить его при разборе папки — это
/// настройка разбора («только быстро», `nonius intake --fast`), а не этого
/// модуля.
pub fn available() -> bool {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CLASSES_ROOT;
        winreg::RegKey::predef(HKEY_CLASSES_ROOT)
            .open_subkey("SldWorks.Application")
            .is_ok()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Попросить SolidWorks записать STEP файла `source` в `target`.
/// Ошибка — причина по-русски, её можно показать человеку.
pub fn export_step(source: &Path, target: &Path) -> Result<(), String> {
    step(source, target, &Ask::plain()).map_err(|failed| match failed {
        Failed::Refused(why) | Failed::Later(why) => why,
        Failed::Cancelled => "остановлено".into(),
    })
}

/// Проверить связь со своим SolidWorks: поднять его, если он не запущен, и
/// спросить, жив ли. Ответ — свой экземпляр (номер процесса и время
/// старта). SolidWorks человека это не трогает.
pub fn ping() -> Result<Own, String> {
    let hub = crate::hub::hub();
    let pinged = hub.run(&Command::Ping, &Ask::plain());
    pinged.map_err(|failed| match failed {
        Failed::Refused(why) | Failed::Later(why) => why,
        Failed::Cancelled => "остановлено".into(),
    })?;
    crate::hub::own().ok_or_else(|| "SolidWorks закрылся".to_string())
}

/// То же с пожеланиями просящего; видно, окончателен ли отказ.
pub(crate) fn step(source: &Path, target: &Path, ask: &Ask) -> Result<(), Failed> {
    // SolidWorks ищет открытую деталь по полному пути.
    let full = |p: &Path| {
        std::path::absolute(p).map_err(|e| Failed::Later(format!("{}: {e}", p.display())))
    };
    let (source, target) = (full(source)?, full(target)?);
    if !source.is_file() {
        return Err(Failed::Later(format!(
            "файл не найден: {}",
            source.display()
        )));
    }
    let step = Command::Step {
        source: &source,
        target: &target,
    };
    crate::hub::hub().run(&step, ask).map(|_| ())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_refused_before_solidworks() {
        let err = export_step(Path::new("нет-такой.sldprt"), Path::new("нет.step"));
        assert!(err.is_err());
    }
}
