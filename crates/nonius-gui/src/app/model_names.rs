//! Как понять, что за деталь в файле: то же имя, то же без хвоста версии.

use std::path::Path;

use crate::source::PartEntry;
use crate::texts::texts;

/// Куда добавится файл.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// В эту деталь новой версией.
    Existing(String),
    /// Новая деталь с этим именем.
    New(String),
    /// Имя не подходит.
    Nothing,
}

/// Деталь с таким же именем: регистр не важен.
pub fn same_name<'a>(name: &str, parts: &'a [PartEntry]) -> Option<&'a PartEntry> {
    let name = name.to_lowercase();
    parts.iter().find(|p| p.name.to_lowercase() == name)
}

/// Деталь, новой версией которой выглядит файл: то же имя или то же без
/// хвоста версии, как `Test V1` и `Test V2`, `плита` и `плита v2`.
pub fn guess<'a>(name: &str, parts: &'a [PartEntry]) -> Option<&'a PartEntry> {
    let base = |text: &str| without_version(text).unwrap_or_else(|| text.to_lowercase());
    same_name(name, parts).or_else(|| parts.iter().find(|p| base(&p.name) == base(name)))
}

/// Имя без хвоста версии: `Test V2`, `вал_rev.3`, `плита ред2`, `кожух (2)`.
/// Просто число хвостом не считается: `втулка 20` и `втулка 30` — разные.
fn without_version(name: &str) -> Option<String> {
    let name = name.trim().to_lowercase();
    let is_number = |c: char| c.is_ascii_digit() || c == '.';
    let copy = name
        .strip_suffix(')')
        .and_then(|s| s.rsplit_once('('))
        .filter(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        .map(|(head, _)| head);
    let head = copy.or_else(|| {
        let head = name.trim_end_matches(is_number);
        let marked = ["версия", "вер", "ред", "rev", "ver", "v"]
            .iter()
            .find_map(|m| head.trim_end_matches([' ', '.']).strip_suffix(m));
        marked.filter(|_| head.len() < name.len())
    })?;
    let head = head.trim_end_matches([' ', '_', '-', '.']);
    (!head.is_empty()).then(|| head.to_string())
}

/// Первая версия детали описания не требует: и так понятно, что это.
pub fn default_message(target: Option<&str>, parts: &[PartEntry]) -> String {
    let saved = target
        .and_then(|name| parts.iter().find(|p| p.name == name))
        .is_some_and(|p| p.latest.is_some());
    if saved {
        String::new()
    } else {
        texts().first_version.to_string()
    }
}

/// Следующая версия детали: `v3`.
pub fn next_version(part: &str, parts: &[PartEntry]) -> String {
    let latest = parts
        .iter()
        .find(|p| p.name == part)
        .and_then(|p| p.latest.as_deref())
        .and_then(|v| v.trim_start_matches('v').parse::<u32>().ok());
    format!("v{}", latest.unwrap_or(0) + 1)
}

/// Файл SolidWorks в деталь, где уже есть STEP: 3D останется прежним.
pub fn step_stays(file: &Path, target: &Place, parts: &[PartEntry]) -> bool {
    let solidworks = file
        .extension()
        .is_some_and(|e| !e.eq_ignore_ascii_case("step") && !e.eq_ignore_ascii_case("stp"));
    let with_step = |name: &String| {
        parts
            .iter()
            .any(|p| p.name == *name && p.model.as_deref() == Some("model.step"))
    };
    solidworks && matches!(target, Place::Existing(name) if with_step(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str, latest: Option<&str>) -> PartEntry {
        PartEntry {
            name: name.into(),
            assembly: false,
            latest: latest.map(str::to_string),
            dirty: false,
            model: Some("model.step".into()),
            stamp: String::new(),
            folder: String::new(),
        }
    }

    #[test]
    fn versions_are_cut_from_names() {
        for (name, base) in [
            ("Test V2", "test"),
            ("вал_rev.3", "вал"),
            ("плита ред2", "плита"),
            ("кожух (2)", "кожух"),
            ("корпус-v1.2", "корпус"),
        ] {
            assert_eq!(without_version(name).as_deref(), Some(base), "{name}");
        }
        for name in ["втулка 20", "кронштейн", "v2", "M8", "рев"] {
            assert_eq!(without_version(name), None, "{name}");
        }
    }

    #[test]
    fn second_file_of_a_part_is_its_next_version() {
        let parts = [part("Test V1", Some("v1")), part("втулка 20", None)];
        let found = |name: &str| guess(name, &parts).map(|p| p.name.as_str());
        assert_eq!(found("Test V2"), Some("Test V1"));
        assert_eq!(found("TEST v1"), Some("Test V1"));
        assert_eq!(found("втулка 30"), None);
        assert_eq!(found("втулка 20 v2"), Some("втулка 20"));
        assert_eq!(found("Test"), Some("Test V1"));
        assert_eq!(next_version("Test V1", &parts), "v2");
        assert_eq!(next_version("втулка 20", &parts), "v1");
    }

    #[test]
    fn solidworks_over_step_warns() {
        let parts = [part("вал", Some("v1"))];
        let place = Place::Existing("вал".into());
        assert!(step_stays(Path::new("вал.SLDPRT"), &place, &parts));
        assert!(!step_stays(Path::new("вал.step"), &place, &parts));
        let new = Place::New("вал2".into());
        assert!(!step_stays(Path::new("вал.sldprt"), &new, &parts));
    }
}
