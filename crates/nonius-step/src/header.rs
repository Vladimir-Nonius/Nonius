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
    ///
    /// Ограничение: вложенные списки в `FILE_SCHEMA` (`(('a','b'))`) читаются
    /// как одна схема — на определение протокола это не влияет.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Полный заголовок SolidWorks: все поля `FILE_NAME`, кириллица в `\X2\`.
    const SAMPLE_FULL: &str = "ISO-10303-21;\nHEADER;\n\
        FILE_DESCRIPTION(('SolidWorks 2020 - Part'),'2;1');\n\
        FILE_NAME('\\X2\\043A0440043E043D0448044204350439043D\\X0\\.STEP','2026-10-26T12:00:00',\
        ('\\X2\\0412043B0430043404380441043B04300432\\X0\\'),('Workshop'),'SwSTEP 2.0','SolidWorks 2020','');\n\
        FILE_SCHEMA (('AUTOMOTIVE_DESIGN { 1 0 10303 214 3 1 1 }'));\nENDSEC;\nDATA;\nENDSEC;\n\
        END-ISO-10303-21;\n";

    /// Латиница с акцентом через `\X\e9` (Latin-1).
    const SAMPLE_LATIN1: &str = "ISO-10303-21;\nHEADER;\n\
        FILE_NAME('pi\\X\\e9ce.stp','2026-01-01',(''),(''),'','DemoCAD','');\n\
        FILE_SCHEMA (('AP203'));\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n";

    /// В DATA есть запись, чья строка содержит слово «FILE_NAME» — помеха поиску.
    const SAMPLE_DATA_NOISE: &str = concat!(
        "ISO-10303-21;\nHEADER;\n",
        "FILE_NAME('x.step','2026-01-01',(''),(''),'tr','AP 214 IS','');\n",
        "FILE_SCHEMA (('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\n",
        "#1=PROPERTY_LIST_VALUE('FILE_NAME','мусор');\nENDSEC;\nEND-ISO-10303-21;\n"
    );

    /// Несколько схем в списке.
    const SAMPLE_MULTI_SCHEMA: &str = concat!(
        "ISO-10303-21;\nHEADER;\n",
        "FILE_NAME('m.stp','2026-01-01',(''),(''),'','CAD','');\n",
        "FILE_SCHEMA (('AP242_IS_MANAGED','AP203'));\n",
        "ENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n"
    );

    #[test]
    fn reads_full_header_fields() {
        let h = StepHeader::parse(SAMPLE_FULL).unwrap();
        assert_eq!(h.name, "кронштейн.STEP");
        assert_eq!(h.timestamp, "2026-10-26T12:00:00");
        assert_eq!(h.authors, ["Владислав"]);
        assert_eq!(h.organizations, ["Workshop"]);
        assert_eq!(h.preprocessor, "SwSTEP 2.0");
        assert_eq!(h.originating_system, "SolidWorks 2020");
    }

    #[test]
    fn decodes_latin1_escapes() {
        // \X\e9 — «é» в Latin-1.
        let h = StepHeader::parse(SAMPLE_LATIN1).unwrap();
        assert_eq!(h.name, "piéce.stp");
    }

    #[test]
    fn finds_entities_after_data_with_same_names() {
        // В DATA есть строка с текстом "FILE_NAME" — заголовок всё равно читается.
        let h = StepHeader::parse(SAMPLE_DATA_NOISE).unwrap();
        assert_eq!(h.originating_system, "AP 214 IS");
    }

    #[test]
    fn parses_multiple_schemas() {
        let h = StepHeader::parse(SAMPLE_MULTI_SCHEMA).unwrap();
        assert_eq!(h.schemas.len(), 2);
        assert_eq!(h.protocol(), Some("AP242"));
    }

    #[test]
    fn read_reports_error_for_missing_file() {
        let err = StepHeader::read(Path::new("/nonius/нет-такого-файла.step")).unwrap_err();
        assert!(err.to_string().contains("нет-такого-файла.step"));
    }

    #[test]
    fn decode_leaves_plain_text_and_unknown_escapes() {
        assert_eq!(decode("обычный текст"), "обычный текст");
        assert_eq!(decode("\\X\\zz"), "\\X\\zz");
        assert_eq!(decode("a\\\\b"), "a\\\\b");
    }

    #[test]
    fn string_value_handles_dollar_and_star() {
        assert_eq!(string_value("$"), "");
        assert_eq!(string_value("*"), "");
        assert_eq!(string_value("'ok'"), "ok");
    }
}
