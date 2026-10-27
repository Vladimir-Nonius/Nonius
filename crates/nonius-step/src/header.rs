//! Разбор заголовка STEP: `FILE_DESCRIPTION`, `FILE_NAME`, `FILE_SCHEMA`.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use nonius_core::{Error, Result};

use crate::StepHeaderSource;

/// Сколько байт читать в поисках заголовка: он всегда в самом начале файла.
const HEADER_LIMIT: u64 = 256 * 1024;

/// Сведения из заголовка STEP.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StepHeader {
    /// Имя файла внутри STEP.
    pub name: String,
    /// Дата записи.
    pub timestamp: String,
    /// Авторы.
    pub authors: Vec<String>,
    /// Организации.
    pub organizations: Vec<String>,
    /// Программа-транслятор (обычно ядро экспорта).
    pub preprocessor: String,
    /// CAD-система, которая сделала файл: «SolidWorks 2020», «KOMPAS-3D».
    pub originating_system: String,
    /// Схемы: `AUTOMOTIVE_DESIGN` (AP214), `AP242_…`, `CONFIG_CONTROL_DESIGN` (AP203).
    pub schemas: Vec<String>,
}

impl StepHeader {
    /// Прочитать заголовок из файла.
    pub fn read(path: &Path) -> Result<Self> {
        let io = |source| Error::Io {
            path: path.to_path_buf(),
            source,
        };
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(io)?
            .take(HEADER_LIMIT)
            .read_to_end(&mut bytes)
            .map_err(io)?;
        Self::parse(&String::from_utf8_lossy(&bytes)).ok_or_else(|| Error::Module {
            module: StepHeaderSource::ID.into(),
            message: format!(
                "{}: это не STEP-файл (нет заголовка ISO-10303-21)",
                path.display()
            ),
        })
    }

    /// Разобрать текст начала файла.
    pub fn parse(text: &str) -> Option<Self> {
        let start = text.find("ISO-10303-21")?;
        let text = &text[start..];
        let end = text.find("ENDSEC").unwrap_or(text.len());
        let header = &text[..end];

        let mut h = Self::default();
        if let Some(args) = entity_args(header, "FILE_NAME") {
            let get = |i: usize| args.get(i).cloned().unwrap_or_default();
            h.name = string_value(&get(0));
            h.timestamp = string_value(&get(1));
            h.authors = list_values(&get(2));
            h.organizations = list_values(&get(3));
            h.preprocessor = string_value(&get(4));
            h.originating_system = string_value(&get(5));
        }
        if let Some(args) = entity_args(header, "FILE_SCHEMA") {
            h.schemas = list_values(args.first().map(String::as_str).unwrap_or_default());
        }
        Some(h)
    }

    /// Протокол словами: AP203, AP214, AP242.
    pub fn protocol(&self) -> Option<&'static str> {
        let s = self.schemas.join(" ").to_ascii_uppercase();
        if s.contains("AP242") {
            Some("AP242")
        } else if s.contains("AUTOMOTIVE_DESIGN") || s.contains("AP214") {
            Some("AP214")
        } else if s.contains("CONFIG_CONTROL_DESIGN") || s.contains("AP203") {
            Some("AP203")
        } else {
            None
        }
    }
}

/// Аргументы записи `NAME(...)` верхнего уровня, разделённые по запятым.
fn entity_args(header: &str, name: &str) -> Option<Vec<String>> {
    let mut search = 0;
    let open = loop {
        let pos = search + header[search..].find(name)?;
        let after = header[pos + name.len()..].trim_start();
        let boundary =
            pos == 0 || !header[..pos].ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_');
        if boundary && after.starts_with('(') {
            break pos + name.len() + (header[pos + name.len()..].len() - after.len());
        }
        search = pos + name.len();
    };

    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut chars = header[open + 1..].chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                current.push(c);
                // строка: '' внутри — экранированная кавычка
                while let Some(s) = chars.next() {
                    current.push(s);
                    if s == '\'' {
                        if chars.peek() == Some(&'\'') {
                            current.push(chars.next().unwrap_or('\''));
                        } else {
                            break;
                        }
                    }
                }
            }
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' if depth == 0 => {
                args.push(current.trim().to_string());
                return Some(args);
            }
            ')' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => args.push(std::mem::take(&mut current).trim().to_string()),
            _ => current.push(c),
        }
    }
    None
}

/// Значение строки STEP: `'abc'` → `abc`, `$` → пусто.
fn string_value(raw: &str) -> String {
    let raw = raw.trim();
    match raw.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        Some(inner) => decode(&inner.replace("''", "'")),
        None => String::new(),
    }
}

/// Список строк: `('a','b')` → `[a, b]`, пустые пропускаются.
fn list_values(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    let Some(inner) = raw.strip_prefix('(').and_then(|r| r.strip_suffix(')')) else {
        return Vec::new();
    };
    entity_args(&format!("L({inner})"), "L")
        .unwrap_or_default()
        .iter()
        .map(|s| string_value(s))
        .filter(|s| !s.trim().is_empty())
        .collect()
}

/// Раскодировать `\X2\…\X0\` (UTF-16) и `\X\hh` (Latin-1) — так STEP
/// записывает кириллицу и другие не-ASCII символы.
fn decode(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(pos) = rest.find('\\') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        if let Some(body) = tail.strip_prefix("\\X2\\") {
            if let Some(end) = body.find("\\X0\\") {
                let hex = &body[..end];
                let units: Vec<u16> = (0..hex.len() / 4)
                    .filter_map(|i| u16::from_str_radix(&hex[i * 4..i * 4 + 4], 16).ok())
                    .collect();
                out.push_str(&String::from_utf16_lossy(&units));
                rest = &body[end + 4..];
                continue;
            }
        }
        if let Some(body) = tail.strip_prefix("\\X\\") {
            if let Some(b) = body.get(..2).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(char::from(b));
                rest = &body[2..];
                continue;
            }
        }
        out.push('\\');
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}
