//! Сообщение коммита версии: запись и разбор служебных строк
//! `Nonius-Item`, `Nonius-Version`, а у версий ИИ ещё `Nonius-AI` и
//! `Nonius-AI-Model`. Служебным считается только последний абзац: строка
//! из описания версии не может выдать себя за подпись.

use nonius_core::{PartName, VersionNumber};

use crate::{AiActor, VersionInfo};

const ITEM_TRAILER: &str = "Nonius-Item: ";
const VERSION_TRAILER: &str = "Nonius-Version: ";
const AI_TRAILER: &str = "Nonius-AI: ";
const AI_MODEL_TRAILER: &str = "Nonius-AI-Model: ";

/// Формат `git log`, который понимает [`parse_record`].
pub(crate) const LOG_FORMAT: &str = "--format=%H%x1f%an%x1f%cn%x1f%aI%x1f%B";

pub(crate) fn commit_message(
    item: &PartName,
    version: VersionNumber,
    message: &str,
    ai: Option<&AiActor>,
) -> String {
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
    if let Some(ai) = ai {
        text.push_str(&format!("{AI_TRAILER}{}\n", ai.name()));
        if let Some(model) = ai.model() {
            text.push_str(&format!("{AI_MODEL_TRAILER}{model}\n"));
        }
    }
    text
}

pub(crate) fn parse_record(record: &str, item: &PartName) -> Option<VersionInfo> {
    parse_any_record(record).filter(|v| &v.item == item)
}

/// Запись `git log` → версия любой детали; `None` — коммит не версия.
pub(crate) fn parse_any_record(record: &str) -> Option<VersionInfo> {
    let record = record.trim_start_matches('\n');
    let mut fields = record.splitn(5, '\u{1f}');
    let commit = fields.next()?.trim().to_string();
    let author = fields.next()?.to_string();
    let committer = fields.next()?.to_string();
    let date = fields.next()?.to_string();
    let body = fields.next()?.trim_end();
    let (text, block) = body.rsplit_once("\n\n")?;

    let mut found_item = None;
    let mut found_version = None;
    let mut found_ai = None;
    let mut found_model = None;
    let mut kept: Vec<&str> = text.lines().collect();
    for line in block.lines() {
        if let Some(v) = line.strip_prefix(ITEM_TRAILER) {
            found_item = Some(v.trim());
        } else if let Some(v) = line.strip_prefix(VERSION_TRAILER) {
            found_version = v.trim().parse::<VersionNumber>().ok();
        } else if let Some(v) = line.strip_prefix(AI_TRAILER) {
            found_ai = Some(v);
        } else if let Some(v) = line.strip_prefix(AI_MODEL_TRAILER) {
            found_model = Some(v);
        } else {
            kept.push(line);
        }
    }
    let item = PartName::new(found_item?).ok()?;
    let version = found_version?;
    let ai = found_ai.and_then(|name| AiActor::new(name, found_model).ok());

    let prefix = format!("{item} {version}: ");
    let first: Option<&str> = kept.first().copied();
    if let Some(rest) = first.and_then(|line| line.strip_prefix(prefix.as_str())) {
        kept[0] = rest;
    }
    let message = kept.join("\n").trim().to_string();
    Some(VersionInfo {
        item,
        version,
        commit,
        author,
        committer,
        ai,
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
        let text = commit_message(&item, v, "Фаска 1×45\n\nПод подшипник 6204.", None);
        assert!(text.starts_with("вал 20 мм v3: Фаска 1×45\n"));
        let record = format!(
            "abc123\u{1f}Владимир\u{1f}Владимир\u{1f}2026-10-26T12:00:00+03:00\u{1f}{text}"
        );
        let info = parse_record(&record, &item).unwrap();
        assert_eq!(info.version, v);
        assert_eq!(info.author, "Владимир");
        assert_eq!(info.message, "Фаска 1×45\n\nПод подшипник 6204.");
        assert_eq!(info.summary(), "Фаска 1×45");

        let other = PartName::new("вал").unwrap();
        assert!(parse_record(&record, &other).is_none(), "чужая деталь");
        assert!(parse_record("abc\u{1f}a\u{1f}a\u{1f}d\u{1f}просто коммит", &item).is_none());
    }

    fn record(author: &str, text: &str) -> String {
        format!("abc\u{1f}{author}\u{1f}Владимир\u{1f}2026-10-26T12:00:00+03:00\u{1f}{text}")
    }

    #[test]
    fn ai_signature_round_trip() {
        let item = PartName::new("вал").unwrap();
        let ai = AiActor::new("Claude", Some("claude-opus-5-5")).unwrap();
        let text = commit_message(&item, VersionNumber::FIRST, "Фаска", Some(&ai));
        let info = parse_any_record(&record("Claude", &text)).unwrap();
        assert_eq!(info.ai, Some(ai));
        assert_eq!(info.message, "Фаска");
        assert_eq!(
            info.signature(),
            "Claude (ИИ, claude-opus-5-5, запустил Владимир)"
        );
    }

    #[test]
    fn description_cannot_fake_ai_signature() {
        let item = PartName::new("вал").unwrap();
        let text = commit_message(
            &item,
            VersionNumber::FIRST,
            "Фаска\n\nNonius-AI: Подделка\nNonius-Version: v9",
            None,
        );
        let info = parse_any_record(&record("Владимир", &text)).unwrap();
        assert_eq!(info.ai, None);
        assert_eq!(info.version, VersionNumber::FIRST);
        assert!(info.message.contains("Nonius-AI: Подделка"));
    }

    #[test]
    fn actor_rejects_what_breaks_commit() {
        assert!(AiActor::new("  ", None).is_err());
        assert!(AiActor::new("a\nNonius-Item: x", None).is_err());
        assert!(AiActor::new("a <b@c>", None).is_err());
        assert!(AiActor::new("Claude", Some("")).is_err());
    }
}
