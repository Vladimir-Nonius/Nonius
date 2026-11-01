//! Windows: программы для типа файла из реестра, запуск, «Открыть с помощью»,
//! показ в Проводнике.
//!
//! Реестр хранит программы двумя способами: тип файла (ProgID) со строкой запуска
//! и имя exe в списке `OpenWithList`. Сначала идёт выбор пользователя.

use std::collections::HashSet;
use std::io::{Error, ErrorKind, Result};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use winreg::enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

use super::Program;

const FILE_EXTS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts";
const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";
const MUI_CACHE: &str =
    r"Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache";

pub(super) fn programs(extension: &str) -> Vec<Program> {
    let dot = format!(".{extension}");
    let classes = RegKey::predef(HKEY_CLASSES_ROOT);
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let chosen = user.open_subkey(format!(r"{FILE_EXTS}\{dot}")).ok();

    let mut commands = Vec::new();
    let progids = chosen
        .iter()
        .flat_map(|k| k.open_subkey("UserChoice"))
        .flat_map(|k| k.get_value::<String, _>("ProgId"))
        .chain(default_value(&classes, &dot))
        .chain(value_names(&classes, &format!(r"{dot}\OpenWithProgids")))
        .chain(
            chosen
                .iter()
                .flat_map(|k| value_names(k, "OpenWithProgids")),
        );
    commands.extend(progids.filter_map(|id| open_command(&classes, &id)));

    if let Some(key) = &chosen {
        commands.extend(recent_exes(key).filter_map(|exe| exe_command(&classes, &exe)));
    }
    for list in [
        format!(r"{dot}\OpenWithList"),
        format!(r"SystemFileAssociations\{dot}\OpenWithList"),
    ] {
        commands.extend(
            subkey_names(&classes, &list)
                .iter()
                .filter_map(|exe| exe_command(&classes, exe)),
        );
    }

    let mut seen = HashSet::new();
    commands
        .into_iter()
        .filter_map(|command| {
            let exe = expand(split(&command).first()?);
            let exists = Path::new(&exe).is_file();
            (exists && seen.insert(exe.to_lowercase())).then(|| Program {
                name: friendly_name(&user, &exe),
                command,
            })
        })
        .collect()
}

pub(super) fn launch(program: &Program, file: &Path) -> Result<()> {
    let args = split(&program.command);
    let Some((exe, rest)) = args.split_first() else {
        return Err(Error::from(ErrorKind::InvalidInput));
    };
    Command::new(expand(exe))
        .args(arguments(rest, &file.to_string_lossy()))
        .spawn()
        .map(drop)
}

pub(super) fn choose(file: &Path) -> Result<()> {
    // Путь нужен без кавычек: rundll32 отдаёт хвост строки как есть.
    Command::new("rundll32.exe")
        .arg("shell32.dll,OpenAs_RunDLL")
        .raw_arg(backslashes(file))
        .spawn()
        .map(drop)
}

pub(super) fn reveal(file: &Path) -> Result<()> {
    Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{}\"", backslashes(file)))
        .spawn()
        .map(drop)
}

fn backslashes(file: &Path) -> String {
    file.to_string_lossy().replace('/', "\\")
}

/// Значение ключа по умолчанию, если ключ есть.
fn default_value(key: &RegKey, path: &str) -> Option<String> {
    key.open_subkey(path).ok()?.get_value("").ok()
}

fn value_names(key: &RegKey, path: &str) -> Vec<String> {
    key.open_subkey(path)
        .map(|k| k.enum_values().flatten().map(|(name, _)| name).collect())
        .unwrap_or_default()
}

fn subkey_names(key: &RegKey, path: &str) -> Vec<String> {
    key.open_subkey(path)
        .map(|k| k.enum_keys().flatten().collect())
        .unwrap_or_default()
}

/// Имена exe из `OpenWithList` пользователя: значения `a`, `b`, …
fn recent_exes(chosen: &RegKey) -> impl Iterator<Item = String> {
    let list = chosen.open_subkey("OpenWithList").ok();
    let names = list.iter().flat_map(|k| k.enum_values().flatten());
    let exes: Vec<String> = names
        .filter(|(name, _)| name != "MRUList" && !name.is_empty())
        .map(|(_, value)| value.to_string())
        .collect();
    exes.into_iter()
}

fn open_command(classes: &RegKey, progid: &str) -> Option<String> {
    let command = default_value(classes, &format!(r"{progid}\shell\open\command"))?;
    (!command.trim().is_empty()).then_some(command)
}

fn exe_command(classes: &RegKey, exe: &str) -> Option<String> {
    open_command(classes, &format!(r"Applications\{exe}")).or_else(|| {
        [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
            .into_iter()
            .find_map(|root| default_value(&RegKey::predef(root), &format!(r"{APP_PATHS}\{exe}")))
            .map(|path| format!("\"{}\" \"%1\"", path.trim_matches('"')))
    })
}

/// Название, которое Windows запомнила для exe, иначе имя файла.
fn friendly_name(user: &RegKey, exe: &str) -> String {
    let remembered = user
        .open_subkey(MUI_CACHE)
        .and_then(|k| k.get_value::<String, _>(format!("{exe}.FriendlyAppName")));
    remembered.unwrap_or_else(|_| {
        let stem = Path::new(exe).file_stem().unwrap_or_default();
        stem.to_string_lossy().into_owned()
    })
}

/// Разбить строку запуска на части: в кавычках пробел не разделяет.
fn split(command: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in command.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            ' ' | '\t' if !quoted => {
                if started {
                    parts.push(std::mem::take(&mut part));
                }
                started = false;
            }
            _ => {
                part.push(c);
                started = true;
            }
        }
    }
    if started {
        parts.push(part);
    }
    parts
}

/// Аргументы с подставленным файлом. Без `%1` файл идёт последним.
fn arguments(template: &[String], file: &str) -> Vec<String> {
    let mut placed = false;
    let mut args: Vec<String> = template
        .iter()
        .filter(|a| a.as_str() != "%*")
        .map(|a| {
            let filled = a
                .replace("%1", file)
                .replace("%L", file)
                .replace("%l", file);
            placed |= filled != *a;
            expand(&filled)
        })
        .collect();
    if !placed {
        args.push(file.to_string());
    }
    args
}

/// Подставить переменные окружения: `%ProgramFiles%\app.exe`.
fn expand(text: &str) -> String {
    let mut result = String::new();
    let mut rest = text;
    while let Some((before, tail)) = rest.split_once('%') {
        result.push_str(before);
        let variable = tail
            .split_once('%')
            .and_then(|(name, after)| Some((std::env::var(name).ok()?, after)));
        match variable {
            Some((value, after)) => {
                result.push_str(&value);
                rest = after;
            }
            None => {
                result.push('%');
                rest = tail;
            }
        }
    }
    result + rest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quoted_command() {
        assert_eq!(
            split(r#""C:\Program Files\app.exe" /open "%1""#),
            [r"C:\Program Files\app.exe", "/open", "%1"]
        );
        assert_eq!(split(r#"app.exe "" x"#), ["app.exe", "", "x"]);
    }

    #[test]
    fn puts_file_into_arguments() {
        let template = ["/open".to_string(), "%1".to_string(), "%*".to_string()];
        assert_eq!(
            arguments(&template, r"C:\а б.dxf"),
            ["/open", r"C:\а б.dxf"]
        );
        assert_eq!(arguments(&["/x".to_string()], "f"), ["/x", "f"]);
    }

    #[test]
    fn expands_only_known_variables() {
        std::env::set_var("NONIUS_TEST_DIR", r"C:\x");
        assert_eq!(expand(r"%NONIUS_TEST_DIR%\a.exe"), r"C:\x\a.exe");
        assert_eq!(expand("100% %нет такой%"), "100% %нет такой%");
    }

    #[test]
    fn unknown_type_has_no_programs() {
        assert!(programs("nonius-unknown-type").is_empty());
    }

    #[test]
    fn launch_passes_file_with_spaces() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("файл с пробелом.txt");
        std::fs::write(&source, "есть").unwrap();
        let target = dir.path().join("копия.txt");
        let args = split(&format!(r#"cmd.exe /c copy "%1" "{}""#, target.display()));
        let (exe, rest) = args.split_first().unwrap();
        let status = Command::new(exe)
            .args(arguments(rest, &source.to_string_lossy()))
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "есть");
    }
}
