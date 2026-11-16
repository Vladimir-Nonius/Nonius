//! Связи файлов SolidWorks: из чего собрана сборка, к какой модели чертёж.
//!
//! Отвечает свой SolidWorks (команда `deps` рабочего процесса, `hub.rs`) —
//! по файлам на диске, не открывая их: миллисекунды на файл. Тот же сеанс и
//! та же очередь, что у выгрузки STEP; к SolidWorks человека Nonius не
//! подключается.
//!
//! То же самое без SolidWorks читает `nonius-sldprt` (`Design`: состав
//! сборки, модель чертежа) — этот модуль его не заменяет и не трогает.

use std::path::{Path, PathBuf};

use nonius_core::extension::{ExtensionPoint, LinkSource, Module, ModuleManifest, API_VERSION};
use nonius_core::{Error, Result};

use crate::hub::{Ask, Hub};
use crate::session::Failed;
use crate::wire::Command;

/// Модуль: связи файлов `.sldasm`, `.slddrw`, `.sldprt` от установленного
/// SolidWorks.
pub struct SolidWorksLinks {
    manifest: ModuleManifest,
}

impl SolidWorksLinks {
    /// Идентификатор модуля.
    pub const ID: &'static str = "solidworks-links";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: ModuleManifest {
                id: Self::ID.into(),
                name: "Связи файлов SolidWorks".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                api: API_VERSION,
                points: vec![ExtensionPoint::Links],
                permissions: vec![],
            },
        }
    }
}

impl Default for SolidWorksLinks {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for SolidWorksLinks {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl LinkSource for SolidWorksLinks {
    fn knows(&self, path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            ["sldasm", "slddrw", "sldprt"]
                .iter()
                .any(|known| e.eq_ignore_ascii_case(known))
        })
    }

    fn links(&self, files: &[PathBuf]) -> Result<Vec<Vec<PathBuf>>> {
        if files.is_empty() {
            return Ok(Vec::new());
        }
        ask(crate::hub::hub(), files).map_err(|message| Error::Module {
            module: Self::ID.into(),
            message,
        })
    }
}

/// Спросить свой SolidWorks про каждый файл. Это фоновая работа: просьбы
/// человека (импорт, «Показать в 3D») идут в очереди вперёд.
///
/// Один нечитаемый файл не роняет разбор всей папки: у него просто нет
/// связей. Ошибка — только когда SolidWorks не ответил ни про один файл
/// (не запустился, не установлен).
fn ask(hub: &Hub, files: &[PathBuf]) -> std::result::Result<Vec<Vec<PathBuf>>, String> {
    let mut found = Vec::with_capacity(files.len());
    let (mut answered, mut problem) = (false, None);
    for (n, file) in files.iter().enumerate() {
        let ask = Ask {
            urgent: false,
            place: Some((n + 1, files.len())),
            ..Ask::plain()
        };
        let mut links: Vec<PathBuf> = Vec::new();
        match hub.run(&Command::Deps(file), &ask) {
            Ok(lines) => {
                answered = true;
                for line in lines {
                    let link = PathBuf::from(line.trim());
                    if !links.contains(&link) {
                        links.push(link);
                    }
                }
            }
            // SolidWorks жив, но про этот файл не ответил.
            Err(Failed::Refused(_)) => answered = true,
            // Не запустился — остальные файлы не ждут: сеанс минуту не
            // пробует снова и отвечает сразу.
            Err(Failed::Later(why)) => problem = Some(why),
            Err(Failed::Cancelled) => return Err("остановлено".into()),
        }
        found.push(links);
    }
    match problem {
        Some(why) if !answered => Err(why),
        _ => Ok(found),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::tests::{line, quick, run, Fake};
    use crate::session::Session;

    #[test]
    fn links_come_from_the_worker_in_question_order() {
        let (fake, log) = Fake::new(vec![run(|command| {
            if command.contains("узел") {
                vec![
                    line("DEP\tC:\\п\\КТМ 640\\вал.SLDPRT"),
                    line("DEP\tC:\\п\\КТМ 640\\втулка.SLDPRT"),
                    line("DEP\tC:\\п\\КТМ 640\\вал.SLDPRT"),
                    line("OK"),
                ]
            } else if command.contains("битый") {
                vec![line("ERROR\t0\tфайл не читается")]
            } else if command.contains("вал.SLDDRW") {
                vec![line("DEP\tC:\\п\\КТМ 640\\вал.SLDPRT"), line("OK")]
            } else {
                vec![line("OK")]
            }
        })]);
        let hub = Hub::new(Session::new(fake, quick()));
        let files = [
            PathBuf::from(r"C:\п\КТМ 640\узел.SLDASM"),
            PathBuf::from(r"C:\п\КТМ 640\вал.SLDPRT"),
            PathBuf::from(r"C:\п\КТМ 640\битый.SLDPRT"),
            PathBuf::from(r"C:\п\КТМ 640\вал.SLDDRW"),
        ];
        let links = ask(&hub, &files).unwrap();
        assert_eq!(links.len(), 4);
        assert_eq!(links[0].len(), 2, "повтор детали в сборке — одна связь");
        assert!(links[1].is_empty());
        assert!(links[2].is_empty(), "нечитаемый файл разбор не роняет");
        assert_eq!(links[3], [PathBuf::from(r"C:\п\КТМ 640\вал.SLDPRT")]);
        let log = log.lock().unwrap();
        assert_eq!(log.launches, 1, "вся папка — один SolidWorks");
        assert!(log.sent.iter().all(|l| l.starts_with("deps\t")));
    }

    #[test]
    fn silent_solidworks_is_an_error_not_an_empty_answer() {
        // SolidWorks не запускается: «связей нет» было бы неправдой.
        let (fake, log) = Fake::new(Vec::new());
        let hub = Hub::new(Session::new(fake, quick()));
        let files: Vec<PathBuf> = (0..40)
            .map(|n| PathBuf::from(format!("{n}.SLDASM")))
            .collect();
        assert!(ask(&hub, &files).unwrap_err().contains("не установлен"));
        assert_eq!(log.lock().unwrap().launches, 1, "сорок раз не пробуем");
    }
}
