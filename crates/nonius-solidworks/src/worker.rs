//! Рабочий процесс как труба: строка туда, строки обратно.
//!
//! [`Worker`] — живой рабочий процесс, [`Launch`] — способ его запустить.
//! Настоящий ([`PowerShell`]) запускает `worker.ps1` без окна; в тестах
//! вместо него поддельный, без процессов вовсе. Всё, что выше (таймауты,
//! перезапуск, очередь), про PowerShell не знает.

use std::time::Duration;

/// Что пришло от рабочего процесса за время ожидания.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Got {
    /// Строка ответа.
    Line(String),
    /// Ничего: время ожидания вышло.
    Quiet,
    /// Рабочий процесс закрылся.
    Closed,
}

/// Живой рабочий процесс.
pub(crate) trait Worker: Send {
    /// Отправить строку. `false` — отправлять некому.
    fn send(&mut self, line: &str) -> bool;
    /// Дождаться строки, но не дольше `wait`.
    fn next(&mut self, wait: Duration) -> Got;
    /// Снять процесс. Свой SolidWorks Windows закроет следом (Job Object).
    fn kill(&mut self);
}

/// Способ запустить рабочий процесс.
pub(crate) trait Launch: Send {
    /// Запустить. Ошибка — причина по-русски.
    fn launch(&self) -> Result<Box<dyn Worker>, String>;
}

/// Настоящий рабочий процесс: `powershell.exe` со скриптом `worker.ps1`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PowerShell {
    /// Без SolidWorks (`-Dry`): для проверки самой трубы.
    pub dry: bool,
}

#[cfg(any(windows, test))]
const WORKER: &str = include_str!("worker.ps1");
#[cfg(any(windows, test))]
const HELPER: &str = include_str!("helper.cs");

#[cfg(windows)]
mod real {
    use std::io::{BufRead, BufReader, Write};
    use std::os::windows::process::CommandExt;
    use std::process::{Child, ChildStdin, Command, Stdio};
    use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
    use std::time::Duration;

    use super::{Got, Launch, PowerShell, Worker, HELPER, WORKER};

    /// Не открывать чёрное окно консоли поверх окна Nonius.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    struct Process {
        child: Child,
        stdin: Option<ChildStdin>,
        lines: Receiver<String>,
        /// Скрипты лежат здесь, пока процесс жив.
        _dir: tempfile::TempDir,
    }

    impl Worker for Process {
        fn send(&mut self, line: &str) -> bool {
            let Some(stdin) = self.stdin.as_mut() else {
                return false;
            };
            let sent = stdin.write_all(format!("{line}\n").as_bytes());
            sent.and_then(|()| stdin.flush()).is_ok()
        }

        fn next(&mut self, wait: Duration) -> Got {
            match self.lines.recv_timeout(wait) {
                Ok(line) => Got::Line(line),
                Err(RecvTimeoutError::Timeout) => Got::Quiet,
                Err(RecvTimeoutError::Disconnected) => Got::Closed,
            }
        }

        fn kill(&mut self) {
            self.stdin = None;
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    impl Drop for Process {
        fn drop(&mut self) {
            self.kill();
        }
    }

    impl Launch for PowerShell {
        fn launch(&self) -> Result<Box<dyn Worker>, String> {
            let dir = tempfile::tempdir().map_err(|e| format!("временная папка: {e}"))?;
            let (script, helper) = (dir.path().join("worker.ps1"), dir.path().join("helper.cs"));
            // Метка в начале файла (BOM): без неё Windows PowerShell читает
            // русские буквы скрипта в старой кодировке и ломается.
            std::fs::write(&script, format!("\u{feff}{WORKER}"))
                .and_then(|()| std::fs::write(&helper, HELPER))
                .map_err(|e| format!("скрипт: {e}"))?;
            let mut command = Command::new("powershell.exe");
            command
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                ])
                .arg("-File")
                .arg(&script)
                .arg("-Helper")
                .arg(&helper)
                .arg("-Parent")
                .arg(std::process::id().to_string());
            if self.dry {
                command.arg("-Dry");
            } else {
                let exe = crate::stamp::exe().ok_or("SolidWorks не установлен")?;
                command.arg("-Exe").arg(exe);
            }
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("PowerShell не запустился: {e}"))?;
            let out = child.stdout.take().ok_or("нет вывода PowerShell")?;
            // Ответ читает отдельная нить: так у ожидания есть срок, а
            // длинный ответ не заполнит трубу.
            let (tx, lines) = channel();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(out);
                let mut raw = Vec::new();
                loop {
                    raw.clear();
                    match reader.read_until(b'\n', &mut raw) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let line = String::from_utf8_lossy(&raw).into_owned();
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                    }
                }
            });
            Ok(Box::new(Process {
                stdin: child.stdin.take(),
                child,
                lines,
                _dir: dir,
            }))
        }
    }
}

#[cfg(not(windows))]
impl Launch for PowerShell {
    fn launch(&self) -> Result<Box<dyn Worker>, String> {
        Err("SolidWorks есть только в Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_keep_their_promises() {
        // Свой экземпляр: запуск самим, объект — из ROT по номеру процесса.
        assert!(HELPER.contains("\"SolidWorks_PID_\""));
        assert!(HELPER.contains("KillOnJobClose"));
        assert!(WORKER.contains("StartupProcessCompleted"));
        assert!(WORKER.contains("Put $script:sw 'Visible' $false"));
        assert!(WORKER.contains("Put $script:sw 'UserControl' $false"));
        // К SolidWorks человека не подключаемся никак.
        for attach in ["New-Object -ComObject", "GetActiveObject("] {
            let code = |text: &str| text.lines().any(|l| is_code(l) && l.contains(attach));
            assert!(!code(WORKER) && !code(HELPER), "{attach}");
        }
        // Общие настройки человека не меняются.
        for global in ["SetUserPreference", "AddInsStartup", "FrameState"] {
            let code = |text: &str| text.lines().any(|l| is_code(l) && l.contains(global));
            assert!(!code(WORKER) && !code(HELPER), "{global}");
        }
    }

    /// Строка скрипта — не комментарий.
    fn is_code(line: &str) -> bool {
        let line = line.trim_start();
        !line.starts_with('#') && !line.starts_with("//")
    }

    /// Настоящий PowerShell без SolidWorks: кириллица и пробелы в путях
    /// доходят туда и обратно.
    #[cfg(windows)]
    #[test]
    fn cyrillic_paths_survive_the_pipe() {
        use crate::wire::{parse, Command, Line};

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("КТМ 640");
        std::fs::create_dir(&dir).unwrap();
        let (source, target) = (dir.join("вал копия.SLDPRT"), dir.join("вал копия.step"));
        std::fs::write(&source, "деталь").unwrap();

        let mut worker = PowerShell { dry: true }.launch().unwrap();
        let answer = |worker: &mut Box<dyn Worker>| loop {
            match worker.next(Duration::from_secs(60)) {
                Got::Line(line) => match parse(&line) {
                    Line::Beat { .. } | Line::Other => {}
                    line => return line,
                },
                other => panic!("рабочий процесс молчит: {other:?}"),
            }
        };
        assert!(matches!(answer(&mut worker), Line::Ready(_)));
        let step = Command::Step {
            source: &source,
            target: &target,
        };
        assert!(worker.send(&step.line().unwrap()));
        assert_eq!(answer(&mut worker), Line::Ok);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "деталь");
        assert!(worker.send(&Command::Deps(&source).line().unwrap()));
        assert_eq!(
            answer(&mut worker),
            Line::Dep(source.to_string_lossy().into_owned())
        );
        assert_eq!(answer(&mut worker), Line::Ok);
        // Конец разговора — рабочий процесс уходит сам.
        assert!(worker.send(crate::wire::QUIT));
        loop {
            match worker.next(Duration::from_secs(30)) {
                Got::Closed => break,
                Got::Line(_) => {}
                Got::Quiet => panic!("рабочий процесс не ушёл"),
            }
        }
    }
}
