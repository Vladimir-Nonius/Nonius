//! Мелочи вывода: значки изменений, номера версий, даты, слова «деталь»/«сборка».

use anyhow::Result;
use nonius_core::compare::ChangeKind;
use nonius_core::meta::ItemKind;
use nonius_core::VersionNumber;

pub fn marker(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Added => "+",
        ChangeKind::Removed => "-",
        ChangeKind::Modified => "~",
        ChangeKind::Moved => "→",
        ChangeKind::Renamed => "✎",
    }
}

/// `v3` или `3` → номер версии.
pub fn parse_version(s: &str) -> Result<VersionNumber> {
    let s = s.trim();
    let text = if s.starts_with('v') {
        s.to_string()
    } else {
        format!("v{s}")
    };
    Ok(text.parse()?)
}

/// `2026-10-26T12:34:56+03:00` → `2026-10-26 12:34`.
pub fn short_date(iso: &str) -> String {
    iso.get(..16).unwrap_or(iso).replacen('T', " ", 1)
}

pub fn kind_word(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Part => "деталь",
        ItemKind::Assembly => "сборка",
    }
}
