//! Имена деталей и номера версий.
//!
//! Имя детали — это имя её папки и часть ссылок в заметках
//! (`[[кронштейн@v3#Фаска1]]`), поэтому в нём запрещены символы, которые
//! ломают пути на Windows/Linux или синтаксис ссылок.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Символы, запрещённые в именах: пути Windows/Linux и синтаксис ссылок.
const FORBIDDEN: &[char] = &[
    '/', '\\', ':', '*', '?', '"', '<', '>', '|', '@', '#', '[', ']', '^',
];

/// Имена, зарезервированные Windows.
const WINDOWS_RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Проверенное имя детали или сборки. Кириллица разрешена.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PartName(String);

impl PartName {
    /// Проверить и создать имя.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let fail = |reason| Error::InvalidName {
            name: name.clone(),
            reason,
        };
        if name.trim().is_empty() {
            return Err(fail("пустое имя"));
        }
        if name.trim() != name {
            return Err(fail("пробелы в начале или конце"));
        }
        if name.starts_with('.') || name.ends_with('.') {
            return Err(fail("точка в начале или конце"));
        }
        if name.chars().count() > 120 {
            return Err(fail("длиннее 120 символов"));
        }
        if name
            .chars()
            .any(|c| FORBIDDEN.contains(&c) || c.is_control())
        {
            return Err(fail(
                "запрещённый символ (нельзя / \\ : * ? \" < > | @ # [ ] ^)",
            ));
        }
        let stem = name.split('.').next().unwrap_or_default().to_lowercase();
        if WINDOWS_RESERVED.contains(&stem.as_str()) {
            return Err(fail("имя зарезервировано Windows"));
        }
        Ok(Self(name))
    }

    /// Имя как строка.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PartName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PartName {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for PartName {
    type Error = Error;
    fn try_from(s: String) -> Result<Self> {
        Self::new(s)
    }
}

impl From<PartName> for String {
    fn from(n: PartName) -> Self {
        n.0
    }
}

/// Номер версии детали: `v1`, `v2`, … Нумерация у каждой детали своя.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct VersionNumber(u32);

impl VersionNumber {
    /// Первая версия.
    pub const FIRST: Self = Self(1);

    /// Создать номер. Версии нумеруются с 1.
    pub fn new(n: u32) -> Result<Self> {
        if n == 0 {
            return Err(Error::InvalidName {
                name: "v0".into(),
                reason: "версии нумеруются с 1",
            });
        }
        Ok(Self(n))
    }

    /// Числовое значение.
    pub fn get(self) -> u32 {
        self.0
    }

    /// Следующая версия.
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for VersionNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

impl FromStr for VersionNumber {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let bad = || Error::InvalidName {
            name: s.to_string(),
            reason: "номер версии пишется как v1, v2, …",
        };
        let digits = s.strip_prefix('v').ok_or_else(bad)?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        Self::new(digits.parse().map_err(|_| bad())?)
    }
}

impl TryFrom<String> for VersionNumber {
    type Error = Error;
    fn try_from(s: String) -> Result<Self> {
        s.parse()
    }
}

impl From<VersionNumber> for String {
    fn from(v: VersionNumber) -> Self {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyrillic_names_are_allowed() {
        assert_eq!(PartName::new("кронштейн").unwrap().as_str(), "кронштейн");
        assert!(PartName::new("вал 20 мм").is_ok());
        assert!(PartName::new("RAIL-00").is_ok());
    }

    #[test]
    fn link_and_path_symbols_are_rejected() {
        for bad in [
            "",
            " ",
            "a/b",
            "a\\b",
            "вал@v2",
            "вал#Фаска",
            "[[x]]",
            ".скрытый",
            "con",
            "COM1.step",
            " вал",
        ] {
            assert!(PartName::new(bad).is_err(), "{bad:?} должно быть отклонено");
        }
    }

    #[test]
    fn version_numbers_round_trip() {
        let v: VersionNumber = "v12".parse().unwrap();
        assert_eq!(v.get(), 12);
        assert_eq!(v.to_string(), "v12");
        assert_eq!(v.next().to_string(), "v13");
        for bad in ["v0", "12", "v", "v1a", "V1", "v-1"] {
            assert!(bad.parse::<VersionNumber>().is_err(), "{bad:?}");
        }
    }
}
