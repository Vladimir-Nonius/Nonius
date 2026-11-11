//! Запуск скриптов PowerShell, которые разговаривают с SolidWorks, и разбор
//! их ответа.
//!
//! Скрипты зашиты в программу и на время работы пишутся во временную папку.
//! PowerShell запускается без окна; последняя строка его вывода — `OK` или
//! `ERROR: причина`, всё до неё — данные ответа.
//!
//! SolidWorks, запущенный нами, после ответа не закрывается сразу: его
//! держит открытым и закрывает после полуминуты простоя сторож `close.ps1`
//! (см. `keep_open`).

use std::path::Path;

/// Выгрузка STEP (см. пояснения в начале скрипта).
#[cfg(any(windows, test))]
const EXPORT_STEP: &str = include_str!("export_step.ps1");

/// Закрытие своего SolidWorks после простоя (см. пояснения в скрипте).
#[cfg(any(windows, test))]
const CLOSE: &str = include_str!("close.ps1");

/// Сколько ждать SolidWorks: холодный запуск и большая сборка — это минуты.
#[cfg(windows)]
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Установлен ли SolidWorks: записан ли он в реестре Windows как программа,
/// которой можно давать команды.
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
    // SolidWorks ищет открытую деталь по полному пути.
    let full = |p: &Path| std::path::absolute(p).map_err(|e| format!("{}: {e}", p.display()));
    let (source, target) = (full(source)?, full(target)?);
    if !source.is_file() {
        return Err(format!("файл не найден: {}", source.display()));
    }
    #[cfg(windows)]
    {
        script(EXPORT_STEP, &[("-Source", &source), ("-Target", &target)]).map(|_| ())
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        Err("SolidWorks есть только в Windows".into())
    }
}

/// Выполнить скрипт с параметрами-путями. Возвращает строки его ответа без
/// последней (`OK`); `ERROR: причина` и всё непонятное — ошибка.
#[cfg(windows)]
pub(crate) fn script(text: &str, args: &[(&str, &Path)]) -> Result<Vec<String>, String> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::Instant;

    /// Не открывать чёрное окно консоли поверх окна Nonius.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    if !available() {
        return Err("SolidWorks не установлен".into());
    }
    // Сначала сторож: он запустит SolidWorks (или подключится к открытому)
    // и не даст ему закрыться между нашими скриптами.
    keep_open();
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
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("SolidWorks не ответил за 5 минут".into());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => return Err(format!("PowerShell: {e}")),
        }
    }
    let output = reader.join().unwrap_or_default();
    touch_stamp();
    answer(&String::from_utf8_lossy(&output))
}

/// Файл-отметка «SolidWorks только что был нужен»: по ней сторож считает
/// простой.
#[cfg(windows)]
fn stamp_file() -> std::path::PathBuf {
    std::env::temp_dir().join("nonius-solidworks.stamp")
}

/// Обновить отметку. В неё пишем текст: пустой файл поверх пустого Windows
/// за новую запись не считает и время не обновляет.
#[cfg(windows)]
fn touch_stamp() -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    std::fs::write(stamp_file(), now.to_string()).is_ok()
}

/// Оставить сторожа (`close.ps1`): он держит SolidWorks запущенным, пока
/// Nonius к нему обращается, и закрывает наш невидимый SolidWorks после
/// полуминуты простоя. Сторож — отдельный процесс: он переживёт и короткую
/// команду `nonius import`. Уже есть один — второй сразу уходит сам. Не
/// вышло его запустить — не беда: работа пойдёт, только медленнее.
#[cfg(windows)]
fn keep_open() {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let stamp = stamp_file();
    let script = std::env::temp_dir().join("nonius-solidworks-close.ps1");
    if !touch_stamp() || std::fs::write(&script, format!("\u{feff}{CLOSE}")).is_err() {
        return;
    }
    let _ = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
        ])
        .arg("-File")
        .arg(&script)
        .arg("-Stamp")
        .arg(&stamp)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
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
    fn script_keeps_its_promises() {
        // Ответ, который разбирает `answer`, и правила вежливости на месте.
        assert!(EXPORT_STEP.contains("Write-Output 'OK'"));
        assert!(EXPORT_STEP.contains("Write-Output \"ERROR: $why\""));
        assert!(EXPORT_STEP.contains("GetSaveFlag"));
        // Закрывается только свой SolidWorks: невидимый, без человека и документов.
        assert!(!EXPORT_STEP.contains("ExitApp"));
        assert!(CLOSE.contains("UserControl") && CLOSE.contains("GetDocumentCount"));
        assert!(
            CLOSE.contains("NoniusSolidWorksKeeper"),
            "сторож один на компьютер"
        );
    }

    #[test]
    fn missing_file_is_refused_before_solidworks() {
        let err = export_step(Path::new("нет-такой.sldprt"), Path::new("нет.step"));
        assert!(err.is_err());
    }
}
