//! Сообщение коммита версии: запись и разбор служебных строк
//! `Nonius-Item` и `Nonius-Version`.

use nonius_core::{PartName, VersionNumber};

use crate::VersionInfo;

const ITEM_TRAILER: &str = "Nonius-Item: ";
const VERSION_TRAILER: &str = "Nonius-Version: ";

pub(crate) fn commit_message(item: &PartName, version: VersionNumber, message: &str) -> String {
    let mut lines = message.lines();
    let subject = lines.next().unwrap_or_default().trim();
    let body = lines.collect::<Vec<_>>().join("\n");
    let mut text = format!("{item} {version}: {subject}\n");
    if !body.trim().is_empty() {
        text.push('\n');
        text.push_str(body.trim());
        text.push('\n');
    }
    text.push_str(&format!(
        "\n{ITEM_TRAILER}{item}\n{VERSION_TRAILER}{version}\n"
    ));
    text
}

pub(crate) fn parse_record(record: &str, item: &PartName) -> Option<VersionInfo> {
    let record = record.trim_start_matches('\n');
    let mut fields = record.splitn(4, '\u{1f}');
    let commit = fields.next()?.trim().to_string();
    let author = fields.next()?.to_string();
    let date = fields.next()?.to_string();
    let body = fields.next()?;

    let mut found_item = None;
    let mut found_version = None;
    let mut kept = Vec::new();
    for line in body.lines() {
        if let Some(v) = line.strip_prefix(ITEM_TRAILER) {
            found_item = Some(v.trim());
        } else if let Some(v) = line.strip_prefix(VERSION_TRAILER) {
            found_version = v.trim().parse::<VersionNumber>().ok();
        } else {
            kept.push(line);
        }
    }
    if found_item? != item.as_str() {
        return None;
    }
    let version = found_version?;

    let prefix = format!("{item} {version}: ");
    let first: Option<&str> = kept.first().copied();
    if let Some(rest) = first.and_then(|line| line.strip_prefix(prefix.as_str())) {
        kept[0] = rest;
    }
    let message = kept.join("\n").trim().to_string();
    Some(VersionInfo {
        item: item.clone(),
        version,
        commit,
        author,
        date,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_round_trip() {
        let item = PartName::new("вал 20 мм").unwrap();
        let v = VersionNumber::new(3).unwrap();
        let text = commit_message(&item, v, "Фаска 1×45\n\nПод подшипник 6204.");
        assert!(text.starts_with("вал 20 мм v3: Фаска 1×45\n"));
        let record = format!("abc123\u{1f}Владимир\u{1f}2026-10-26T12:00:00+03:00\u{1f}{text}");
        let info = parse_record(&record, &item).unwrap();
        assert_eq!(info.version, v);
        assert_eq!(info.author, "Владимир");
        assert_eq!(info.message, "Фаска 1×45\n\nПод подшипник 6204.");
        assert_eq!(info.summary(), "Фаска 1×45");

        let other = PartName::new("вал").unwrap();
        assert!(parse_record(&record, &other).is_none(), "чужая деталь");
        assert!(parse_record("abc\u{1f}a\u{1f}d\u{1f}просто коммит", &item).is_none());
    }
}
