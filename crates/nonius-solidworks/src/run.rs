//! Просьбы к SolidWorks: есть ли он, выгрузка STEP, запуск скриптов.
//!
//! STEP выгружает **свой** экземпляр SolidWorks: его запускает рабочий
//! процесс (`worker.ps1`, труба к нему — `worker.rs`, слова — `wire.rs`).
//! К SolidWorks, открытому человеком, Nonius не подключается.
//!
//! Скрипт связей (`links.ps1`) пока запускается по-старому, отдельным
//! PowerShell: последняя строка его вывода — `OK` или `ERROR: причина`.

use std::path::Path;
use std::time::Duration;

use crate::hub::Ask;
use crate::session::Failed;
use crate::wire::Command;

/// Сколько ждать SolidWorks: холодный запуск и большая сборка — это минуты.
const TIMEOUT: Duration = Duration::from_secs(300);

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
/// Выполнить скрипт с параметрами-путями. Возвращает строки его ответа без
/// последней (`OK`); `ERROR: причина` и всё непонятное — ошибка.
#[cfg(windows)]
pub(crate) fn script(text: &str, args: &[(&str, &Path)]) -> Result<Vec<String>, String> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    /// Не открывать чёрное окно консоли поверх окна Nonius.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    if !available() {
        return Err("SolidWorks не установлен".into());
    }
    let dir = tempfile::tempdir().map_err(|e| format!("временная папка: {e}"))?;
    let file = dir.path().join("nonius-solidworks.ps1");
    // Метка в начале файла (BOM): без неё Windows PowerShell читает русские
    // буквы скрипта в старой кодировке и ломается на первой же строке.
    std::fs::write(&file, format!("\u{feff}{text}")).map_err(|e| format!("скрипт: {e}"))?;

    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
        ])
        .arg("-File")
        .arg(&file);
    for (name, value) in args {
        command.arg(name).arg(value);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("PowerShell не запустился: {e}"))?;

    // Ответ читаем сразу, отдельным потоком: длинный ответ иначе заполнил бы
    // трубу, и скрипт встал бы, не дописав.
    let mut out = child.stdout.take().ok_or("нет вывода PowerShell")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = out.read_to_end(&mut bytes);
        bytes
    });
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("SolidWorks не ответил за 5 минут".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("PowerShell: {e}")),
        }
    }
    let output = reader.join().unwrap_or_default();
    answer(&String::from_utf8_lossy(&output))
}

/// Разобрать ответ скрипта: последняя непустая строка — `OK` или
/// `ERROR: причина`; при `OK` возвращаются строки до неё.
#[cfg(any(windows, test))]
fn answer(output: &str) -> Result<Vec<String>, String> {
    let mut lines: Vec<String> = output
        .lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .filter(|l| !l.trim().is_empty())
        .collect();
    match lines.pop().as_deref().map(str::trim) {
        Some("OK") => Ok(lines),
        Some(line) => match line.strip_prefix("ERROR:") {
            Some(why) => Err(why.trim().to_string()),
            None => Err(format!("непонятный ответ SolidWorks: {line}")),
        },
        None => Err("SolidWorks ничего не ответил".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_is_the_last_line() {
        assert_eq!(answer("OK\r\n"), Ok(vec![]));
        assert_eq!(answer("данные\nOK\n\n"), Ok(vec!["данные".to_string()]));
        assert_eq!(
            answer("OK\nERROR: деталь не сохранена\n"),
            Err("деталь не сохранена".into())
        );
        assert!(answer("").unwrap_err().contains("ничего"));
        assert!(answer("что-то пошло не так")
            .unwrap_err()
            .contains("что-то"));
    }

    #[test]
    fn missing_file_is_refused_before_solidworks() {
        let err = export_step(Path::new("нет-такой.sldprt"), Path::new("нет.step"));
        assert!(err.is_err());
    }
}
