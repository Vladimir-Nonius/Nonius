//! Язык разговора с рабочим процессом (`worker.ps1`): команды, строки
//! ответа и что значат коды ошибок COM. Только слова — без процессов и
//! времени, поэтому проверяется целиком без SolidWorks.
//!
//! Строки — UTF-8, поля через табуляцию (в путях бывают пробелы и
//! кириллица). Что на что отвечает — в начале `worker.ps1`.

use std::path::Path;

/// Команда рабочему процессу.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command<'a> {
    /// Жив ли SolidWorks.
    Ping,
    /// Выгрузить STEP файла `source` в `target`.
    Step {
        /// Файл SolidWorks.
        source: &'a Path,
        /// Куда писать STEP.
        target: &'a Path,
    },
    /// На какие файлы ссылается этот файл.
    Deps(&'a Path),
}

impl Command<'_> {
    /// Строка команды. Ошибка — путь, который строкой не передать.
    pub fn line(&self) -> Result<String, String> {
        let cell = |path: &Path| {
            let text = path.to_str().ok_or("в пути есть знаки не из Юникода")?;
            if text.contains(['\t', '\n', '\r']) {
                return Err("в пути есть табуляция или перевод строки");
            }
            Ok(text.to_string())
        };
        let line = match self {
            Self::Ping => Ok("ping".to_string()),
            Self::Step { source, target } => {
                cell(source).and_then(|s| Ok(format!("step\t{s}\t{}", cell(target)?)))
            }
            Self::Deps(file) => cell(file).map(|f| format!("deps\t{f}")),
        };
        line.map_err(|why| format!("{why}: SolidWorks такой файл не передать"))
    }

    /// Файл, над которым работает команда.
    pub fn subject(&self) -> Option<&Path> {
        match self {
            Self::Ping => None,
            Self::Step { source, .. } => Some(source),
            Self::Deps(file) => Some(file),
        }
    }
}

/// Просьба закрыть свой SolidWorks и уйти.
pub(crate) const QUIT: &str = "quit";

/// Строка от рабочего процесса.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Line {
    /// Свой SolidWorks запущен и готов.
    Ready(Own),
    /// Команда выполнена.
    Ok,
    /// Команда не выполнена: код ошибки COM (0 — ошибка не COM) и причина.
    Error {
        /// Код.
        code: u32,
        /// Причина словами.
        why: String,
    },
    /// Строка данных ответа: файл, на который ссылаются.
    Dep(String),
    /// Пульс: SolidWorks жив.
    Beat {
        /// Сколько времени процессора он потратил, мс.
        cpu_ms: u64,
        /// Заголовок его окна-сообщения, если такое появилось.
        dialog: Option<String>,
    },
    /// Процесса SolidWorks больше нет.
    Gone,
    /// Что-то непонятное: пропускается.
    Other,
}

/// Свой экземпляр SolidWorks: по номеру процесса и времени старта его не
/// спутать с SolidWorks человека.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Own {
    /// Номер процесса.
    pub pid: u32,
    /// Время старта процесса (FILETIME, UTC): номера процессов Windows
    /// выдаёт повторно, пара «номер и время» — нет.
    pub started: i64,
    /// Версия SolidWorks, как он её называет: `33.5.0` — это 2025 SP5.
    pub revision: String,
    /// В группе процессов (Job Object): не станет Nonius — Windows закроет
    /// и этот SolidWorks.
    pub fenced: bool,
}

/// Разобрать строку рабочего процесса.
pub(crate) fn parse(line: &str) -> Line {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut cells = line.split('\t');
    match cells.next().unwrap_or("") {
        "OK" => Line::Ok,
        "GONE" => Line::Gone,
        "ERROR" => {
            let code = cells.next().unwrap_or("0").trim_start_matches("0x");
            Line::Error {
                code: u32::from_str_radix(code, 16).unwrap_or(0),
                why: cells.collect::<Vec<_>>().join(" ").trim().to_string(),
            }
        }
        "DEP" => Line::Dep(cells.collect::<Vec<_>>().join("\t")),
        "BEAT" => {
            let cpu_ms = cells.next().and_then(|c| c.parse().ok());
            let dialog = cells.next().map(str::trim).filter(|d| !d.is_empty());
            match cpu_ms {
                Some(cpu_ms) => Line::Beat {
                    cpu_ms,
                    dialog: dialog.map(str::to_string),
                },
                None => Line::Other,
            }
        }
        "READY" => {
            let mut number = || cells.next().and_then(|c| c.parse::<i64>().ok());
            let (Some(pid), Some(started)) = (number(), number()) else {
                return Line::Other;
            };
            Line::Ready(Own {
                pid: u32::try_from(pid).unwrap_or(0),
                started,
                revision: cells.next().unwrap_or("").to_string(),
                fenced: cells.next() == Some("job"),
            })
        }
        _ => Line::Other,
    }
}

/// Что значит код ошибки COM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fault {
    /// SolidWorks умер: нужен перезапуск.
    Dead,
    /// SolidWorks занят: повторить чуть позже.
    Busy,
    /// Обычный отказ: SolidWorks жив и ответил «не могу».
    Plain,
}

/// `RPC_E_DISCONNECTED`, «сервер RPC недоступен», `RPC_E_SERVER_DIED` —
/// умер; `RPC_E_CALL_REJECTED`, `RPC_E_SERVERCALL_RETRYLATER` — занят.
pub(crate) fn fault(code: u32) -> Fault {
    match code {
        0x8001_0108 | 0x8007_06BA | 0x8001_0007 => Fault::Dead,
        0x8001_0001 | 0x8001_010A => Fault::Busy,
        _ => Fault::Plain,
    }
}

/// Окно-сообщение — это отчёт об ошибке после вылета, а не вопрос.
pub(crate) fn crash_report(title: &str) -> bool {
    let title = title.to_lowercase();
    [
        "error report",
        "отчет об ошиб",
        "отчёт об ошиб",
        "crash",
        "сбой",
    ]
    .iter()
    .any(|mark| title.contains(mark))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_tab_separated_lines() {
        let (source, target) = (
            Path::new(r"C:\Documents\КТМ 640\вал копия.SLDPRT"),
            Path::new(r"C:\Temp\model.step"),
        );
        let step = Command::Step { source, target };
        assert_eq!(
            step.line().unwrap(),
            "step\tC:\\Documents\\КТМ 640\\вал копия.SLDPRT\tC:\\Temp\\model.step"
        );
        assert_eq!(step.subject(), Some(source));
        assert_eq!(
            Command::Deps(source).line().unwrap().matches('\t').count(),
            1
        );
        assert_eq!(Command::Ping.line().unwrap(), "ping");
        // Путь, который сломал бы разбор строки, не отправляется.
        let bad = Command::Deps(Path::new("вал\tкопия.sldprt"));
        assert!(bad.line().unwrap_err().contains("табуляция"));
    }

    #[test]
    fn worker_lines_are_understood() {
        assert_eq!(parse("OK\r\n"), Line::Ok);
        assert_eq!(parse("GONE"), Line::Gone);
        assert_eq!(
            parse("ERROR\t0x800706BA\tСервер RPC недоступен"),
            Line::Error {
                code: 0x8007_06BA,
                why: "Сервер RPC недоступен".into()
            }
        );
        assert_eq!(
            parse("ERROR\t0\tSolidWorks не открыл файл (код 2)"),
            Line::Error {
                code: 0,
                why: "SolidWorks не открыл файл (код 2)".into()
            }
        );
        assert_eq!(
            parse("DEP\tC:\\п\\КТМ 640\\вал.SLDPRT"),
            Line::Dep("C:\\п\\КТМ 640\\вал.SLDPRT".into())
        );
        assert_eq!(
            parse("BEAT\t15300\t"),
            Line::Beat {
                cpu_ms: 15300,
                dialog: None
            }
        );
        assert_eq!(
            parse("BEAT\t15300\tSOLIDWORKS"),
            Line::Beat {
                cpu_ms: 15300,
                dialog: Some("SOLIDWORKS".into())
            }
        );
        assert_eq!(
            parse("READY\t6784\t134392310123356634\t33.5.0\tjob"),
            Line::Ready(Own {
                pid: 6784,
                started: 134_392_310_123_356_634,
                revision: "33.5.0".into(),
                fenced: true
            })
        );
        assert_eq!(parse("что-то ещё"), Line::Other);
        assert_eq!(parse("BEAT\tмного"), Line::Other);
    }

    #[test]
    fn com_codes_say_dead_or_busy() {
        for dead in [0x8001_0108, 0x8007_06BA, 0x8001_0007] {
            assert_eq!(fault(dead), Fault::Dead);
        }
        for busy in [0x8001_0001, 0x8001_010A] {
            assert_eq!(fault(busy), Fault::Busy);
        }
        assert_eq!(fault(0), Fault::Plain);
        assert_eq!(fault(0x8002_0009), Fault::Plain);
        assert!(crash_report("SOLIDWORKS Error Report"));
        assert!(!crash_report("SOLIDWORKS"));
    }
}
