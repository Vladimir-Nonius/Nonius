//! Запуск скрипта `export_step.ps1` и разбор его ответа.
//!
//! Скрипт зашит в программу и на время работы пишется во временную папку.
//! PowerShell запускается без окна; ответ — последняя строка его вывода:
//! `OK` или `ERROR: причина`.

use std::path::Path;

/// Сам скрипт (см. пояснения в его начале).
#[cfg(any(windows, test))]
const SCRIPT: &str = include_str!("export_step.ps1");

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
#[cfg(windows)]
pub fn export_step(source: &Path, target: &Path) -> Result<(), String> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::Instant;

    /// Не открывать чёрное окно консоли поверх окна Nonius.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // SolidWorks ищет открытую деталь по полному пути.
    let full = |p: &Path| std::path::absolute(p).map_err(|e| format!("{}: {e}", p.display()));
    let (source, target) = (full(source)?, full(target)?);
    if !source.is_file() {
        return Err(format!("файл не найден: {}", source.display()));
    }
    if !available() {
        return Err("SolidWorks не установлен".into());
    }

    let dir = tempfile::tempdir().map_err(|e| format!("временная папка: {e}"))?;
    let script = dir.path().join("export_step.ps1");
    // Метка в начале файла (BOM): без неё Windows PowerShell читает русские
    // буквы скрипта в старой кодировке и ломается на первой же строке.
    std::fs::write(&script, format!("\u{feff}{SCRIPT}")).map_err(|e| format!("скрипт: {e}"))?;

    let mut child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
        ])
        .arg("-File")
        .arg(&script)
        .arg("-Source")
        .arg(&source)
        .arg("-Target")
        .arg(&target)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("PowerShell не запустился: {e}"))?;

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("SolidWorks не ответил за 5 минут".into());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => return Err(format!("PowerShell: {e}")),
        }
    }
    let mut output = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_end(&mut output);
    }
    answer(&String::from_utf8_lossy(&output))
}

/// Вне Windows SolidWorks не бывает.
#[cfg(not(windows))]
pub fn export_step(_source: &Path, _target: &Path) -> Result<(), String> {
    Err("SolidWorks есть только в Windows".into())
}

/// Разобрать ответ скрипта: последняя непустая строка — `OK` или
/// `ERROR: причина`.
#[cfg(any(windows, test))]
fn answer(output: &str) -> Result<(), String> {
    let last = output.lines().map(str::trim).rfind(|l| !l.is_empty());
    match last {
        Some("OK") => Ok(()),
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
        assert_eq!(answer("OK\r\n"), Ok(()));
        assert_eq!(answer("шум\nOK\n\n"), Ok(()));
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
        assert!(SCRIPT.contains("Write-Output 'OK'"));
        assert!(SCRIPT.contains("Write-Output \"ERROR: $why\""));
        assert!(SCRIPT.contains("GetSaveFlag"));
        assert!(SCRIPT.contains("UserControl"));
    }

    #[test]
    fn missing_file_is_refused_before_solidworks() {
        let err = export_step(Path::new("нет-такой.sldprt"), Path::new("нет.step"));
        assert!(err.is_err());
    }
}
