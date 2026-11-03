//! Сообщение коммита версии: запись и разбор служебных строк
//! `Nonius-Item`, `Nonius-Version`, а у версий ИИ ещё `Nonius-AI` и
//! `Nonius-AI-Model`. Служебным считается только последний абзац: строка
//! из описания версии не может выдать себя за подпись.
//!
//! Переименование, удаление и возврат детали — не версии, а отметки в истории:
//! `Nonius-Renamed`, `Nonius-Deleted`, `Nonius-Recovered`. Так же отмечается,
//! что версию убрали с карты или вернули на неё: `Nonius-Hidden`, `Nonius-Shown`.
//!
//! Деталь перенесли в другую папку: `Nonius-Moved` с прежним путём. У
//! удалённой детали `Nonius-Folder` — где она лежала, у возвращённой в другое
//! место — `Nonius-Moved`. Проект перевели со старых `parts/` и `assemblies/`
//! на папки: `Nonius-Layout: 2`; до этой записи каждая деталь лежала там.

use nonius_core::meta::ItemKind;
use nonius_core::{PartName, VersionNumber};

use crate::{AiActor, DeletedItem, VersionInfo};

const ITEM_TRAILER: &str = "Nonius-Item: ";
const VERSION_TRAILER: &str = "Nonius-Version: ";
const AI_TRAILER: &str = "Nonius-AI: ";
const AI_MODEL_TRAILER: &str = "Nonius-AI-Model: ";
const RENAMED_TRAILER: &str = "Nonius-Renamed: ";
const DELETED_TRAILER: &str = "Nonius-Deleted: ";
const RECOVERED_TRAILER: &str = "Nonius-Recovered: ";
const KIND_TRAILER: &str = "Nonius-Kind: ";
const HIDDEN_TRAILER: &str = "Nonius-Hidden: ";
const SHOWN_TRAILER: &str = "Nonius-Shown: ";
const MOVED_TRAILER: &str = "Nonius-Moved: ";
const FOLDER_TRAILER: &str = "Nonius-Folder: ";
const LAYOUT_TRAILER: &str = "Nonius-Layout: ";

/// Значение `Nonius-Kind` у сборки; у детали строки нет.
const ASSEMBLY: &str = "assembly";

/// Формат `git log`, который понимает [`parse_event`].
pub(crate) const LOG_FORMAT: &str = "--format=%H%x1f%an%x1f%cn%x1f%aI%x1f%B";

/// Отбор `git log`: только то, что меняет историю деталей.
pub(crate) const LOG_EVENTS: [&str; 8] = [
    "--grep=^Nonius-Version: ",
    "--grep=^Nonius-Renamed: ",
    "--grep=^Nonius-Deleted: ",
    "--grep=^Nonius-Recovered: ",
    "--grep=^Nonius-Hidden: ",
    "--grep=^Nonius-Shown: ",
    "--grep=^Nonius-Moved: ",
    "--grep=^Nonius-Layout: ",
];

/// Запись истории проекта.
pub(crate) enum Event {
    /// Версия детали.
    Version(VersionInfo),
    /// Деталь переименована.
    Renamed {
        /// Прежнее имя.
        from: PartName,
        /// Новое имя.
        to: PartName,
    },
    /// Деталь удалена.
    Deleted(DeletedItem),
    /// Удалённая деталь возвращена; `from` — где она лежала, если вернулась
    /// в другое место.
    Recovered {
        /// Имя.
        name: PartName,
        /// Прежняя папка, путь от корня.
        from: Option<String>,
    },
    /// Деталь перенесена в другую папку.
    Moved {
        /// Деталь.
        item: PartName,
        /// Прежняя папка, путь от корня.
        from: String,
    },
    /// Проект переведён со старых `parts/` и `assemblies/` на папки.
    Layout,
    /// Версию убрали с карты (`hidden`) или вернули на неё.
    Hidden {
        /// Деталь под именем на тот момент.
        item: PartName,
        /// Версия.
        version: VersionNumber,
        /// Убрана.
        hidden: bool,
    },
}

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

/// Подпись ИИ в конце сообщения отметки.
fn signed(mut text: String, ai: Option<&AiActor>) -> String {
    if let Some(ai) = ai {
        text.push_str(&format!("{AI_TRAILER}{}\n", ai.name()));
        if let Some(model) = ai.model() {
            text.push_str(&format!("{AI_MODEL_TRAILER}{model}\n"));
        }
    }
    text
}

/// Сообщение отметки «деталь переименована».
pub(crate) fn renamed_message(from: &PartName, to: &PartName, ai: Option<&AiActor>) -> String {
    let text =
        format!("{from} → {to}: новое имя детали\n\n{RENAMED_TRAILER}{from}\n{ITEM_TRAILER}{to}\n");
    signed(text, ai)
}

/// Сообщение отметки «деталь удалена»; `folder` — где она лежала.
pub(crate) fn deleted_message(
    item: &PartName,
    kind: ItemKind,
    folder: &str,
    ai: Option<&AiActor>,
) -> String {
    let mut text = format!("{item}: деталь удалена\n\n{DELETED_TRAILER}{item}\n");
    if kind == ItemKind::Assembly {
        text.push_str(&format!("{KIND_TRAILER}{ASSEMBLY}\n"));
    }
    text.push_str(&format!("{FOLDER_TRAILER}{folder}\n"));
    signed(text, ai)
}

/// Сообщение отметки «деталь возвращена»; `from` — где она лежала, если
/// вернулась в другое место.
pub(crate) fn recovered_message(
    item: &PartName,
    from: Option<&str>,
    ai: Option<&AiActor>,
) -> String {
    let mut text = format!("{item}: деталь возвращена\n\n{RECOVERED_TRAILER}{item}\n");
    if let Some(from) = from {
        text.push_str(&format!("{MOVED_TRAILER}{from}\n"));
    }
    signed(text, ai)
}

/// Сообщение отметки «деталь перенесена»: из папки `from` в `to`.
pub(crate) fn moved_message(item: &PartName, from: &str, to: &str, ai: Option<&AiActor>) -> String {
    let place = match to.rsplit_once('/') {
        Some((parent, _)) => format!("«{parent}»"),
        None => "корень проекта".into(),
    };
    let text =
        format!("{item}: перенесена в {place}\n\n{MOVED_TRAILER}{from}\n{ITEM_TRAILER}{item}\n");
    signed(text, ai)
}

/// Сообщение записи «проект переведён на папки».
pub(crate) fn layout_message(count: usize) -> String {
    format!(
        "Проект переведён на папки: {count} деталей и сборок перенесены в корень\n\n\
         Детали больше не делятся на parts/ и assemblies/: дерево проекта — это\n\
         папки на диске.\n\n{LAYOUT_TRAILER}2\n"
    )
}

/// Сообщение отметки «версия убрана с карты» или «снова на карте».
pub(crate) fn hidden_message(
    item: &PartName,
    version: VersionNumber,
    hidden: bool,
    ai: Option<&AiActor>,
) -> String {
    let (word, trailer) = if hidden {
        ("убрана с карты", HIDDEN_TRAILER)
    } else {
        ("снова на карте", SHOWN_TRAILER)
    };
    signed(
        format!("{item} {version}: {word}\n\n{trailer}{item}@{version}\n"),
        ai,
    )
}

/// `деталь@vN` из отметки карты.
fn parse_mark(text: &str) -> Option<(PartName, VersionNumber)> {
    let (item, version) = text.trim().rsplit_once('@')?;
    Some((PartName::new(item).ok()?, version.parse().ok()?))
}

/// Запись `git log` → событие истории; `None` — обычный коммит.
pub(crate) fn parse_event(record: &str) -> Option<Event> {
    if let Some(v) = parse_any_record(record) {
        return Some(Event::Version(v));
    }
    let record = record.trim_start_matches('\n');
    let mut fields = record.splitn(5, '\u{1f}');
    let commit = fields.next()?.trim().to_string();
    let author = fields.next()?.to_string();
    let _committer = fields.next()?;
    let date = fields.next()?.to_string();
    let (_, block) = fields.next()?.trim_end().rsplit_once("\n\n")?;
    let value = |trailer: &str| {
        let found = block.lines().find_map(|l| l.strip_prefix(trailer))?;
        PartName::new(found.trim()).ok()
    };
    for (trailer, hidden) in [(HIDDEN_TRAILER, true), (SHOWN_TRAILER, false)] {
        let mark = block.lines().find_map(|l| l.strip_prefix(trailer));
        if let Some((item, version)) = mark.and_then(parse_mark) {
            return Some(Event::Hidden {
                item,
                version,
                hidden,
            });
        }
    }
    let path = |trailer: &str| {
        let found = block.lines().find_map(|l| l.strip_prefix(trailer))?;
        Some(found.trim().to_string()).filter(|p| !p.is_empty())
    };
    if block.lines().any(|l| l.starts_with(LAYOUT_TRAILER)) {
        return Some(Event::Layout);
    }
    if let Some(from) = value(RENAMED_TRAILER) {
        let to = value(ITEM_TRAILER)?;
        return Some(Event::Renamed { from, to });
    }
    if let Some(name) = value(RECOVERED_TRAILER) {
        let from = path(MOVED_TRAILER);
        return Some(Event::Recovered { name, from });
    }
    if let Some(from) = path(MOVED_TRAILER) {
        let item = value(ITEM_TRAILER)?;
        return Some(Event::Moved { item, from });
    }
    let name = value(DELETED_TRAILER)?;
    let assembly = block
        .lines()
        .any(|l| l.strip_prefix(KIND_TRAILER).map(str::trim) == Some(ASSEMBLY));
    Some(Event::Deleted(DeletedItem {
        name,
        kind: if assembly {
            ItemKind::Assembly
        } else {
            ItemKind::Part
        },
        folder: path(FOLDER_TRAILER),
        commit,
        author,
        date,
    }))
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
        folder: String::new(),
        commit,
        author,
        committer,
        ai,
        date,
        message,
        hidden: false,
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
        let info = parse_any_record(&record).unwrap();
        assert_eq!(info.item, item);
        assert_eq!(info.version, v);
        assert_eq!(info.author, "Владимир");
        assert_eq!(info.message, "Фаска 1×45\n\nПод подшипник 6204.");
        assert_eq!(info.summary(), "Фаска 1×45");

        assert!(parse_any_record("abc\u{1f}a\u{1f}a\u{1f}d\u{1f}просто коммит").is_none());
    }

    #[test]
    fn marks_round_trip() {
        let a = PartName::new("вал").unwrap();
        let b = PartName::new("ось").unwrap();
        let ai = AiActor::new("Claude", None).unwrap();
        let renamed = record("Claude", &renamed_message(&a, &b, Some(&ai)));
        assert!(
            matches!(parse_event(&renamed), Some(Event::Renamed { from, to }) if from == a && to == b)
        );
        let deleted = record(
            "Я",
            &deleted_message(&a, ItemKind::Assembly, "Стена/вал", None),
        );
        match parse_event(&deleted) {
            Some(Event::Deleted(d)) => {
                assert_eq!(d.name, a);
                assert_eq!(d.kind, ItemKind::Assembly);
                assert_eq!(d.folder.as_deref(), Some("Стена/вал"));
            }
            _ => panic!("не отметка удаления"),
        }
        let back = record("Я", &recovered_message(&a, None, None));
        assert!(
            matches!(parse_event(&back), Some(Event::Recovered { name, from: None }) if name == a)
        );
        let back = record("Я", &recovered_message(&a, Some("parts/вал"), None));
        assert!(matches!(
            parse_event(&back),
            Some(Event::Recovered { from: Some(f), .. }) if f == "parts/вал"
        ));
        let moved = record("Я", &moved_message(&a, "вал", "Стена/вал", None));
        assert!(moved.contains("перенесена в «Стена»"));
        assert!(matches!(
            parse_event(&moved),
            Some(Event::Moved { item, from }) if item == a && from == "вал"
        ));
        assert!(matches!(
            parse_event(&record("Я", &layout_message(3))),
            Some(Event::Layout)
        ));
        // Строка в описании версии отметкой не становится.
        let text = commit_message(&a, VersionNumber::FIRST, "x\n\nNonius-Deleted: вал", None);
        assert!(matches!(
            parse_event(&record("Я", &text)),
            Some(Event::Version(_))
        ));
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
