//! Какие модели — одна деталь, и как её назвать.
//!
//! `вал`, `вал_v2`, `вал (3)` — версии одной детали; но файлы, которые
//! вместе стоят в одной сборке, — разные детали, как бы они ни назывались.

use std::collections::{BTreeMap, BTreeSet};

use nonius_core::PartName;

use super::names::{parse, Mark};
use super::plan::Unit;
use super::LeftFile;

/// Будущая деталь: одна модель или несколько версий одной.
pub(super) struct Proto {
    pub members: Vec<usize>,
    pub base: String,
    pub guessed: bool,
    pub name: Option<PartName>,
}

/// Собрать версии одной детали по именам: `вал`, `вал_v2`, `вал (3)`.
/// Файлы, которые вместе стоят в одной сборке, — разные детали.
pub(super) fn group(units: &[Unit], used_by: &[BTreeSet<usize>]) -> Vec<Proto> {
    let mut by_base: BTreeMap<(String, bool, String), Vec<usize>> = BTreeMap::new();
    for (i, u) in units.iter().enumerate() {
        let base = parse(&u.stem).base.to_lowercase();
        by_base
            .entry((u.dir.clone(), u.is_assembly(), base))
            .or_default()
            .push(i);
    }
    let single = |i: usize| Proto {
        members: vec![i],
        base: units[i].stem.clone(),
        guessed: false,
        name: None,
    };
    let mut protos = Vec::new();
    for mut members in by_base.into_values() {
        if members.len() < 2 {
            protos.push(single(members[0]));
            continue;
        }
        let together = members.iter().enumerate().any(|(n, &a)| {
            members[n + 1..]
                .iter()
                .any(|&b| used_by[a].intersection(&used_by[b]).next().is_some())
        });
        if together {
            protos.extend(members.into_iter().map(single));
            continue;
        }
        let marks: Vec<Option<Mark>> = members
            .iter()
            .map(|&i| parse(&units[i].stem).mark)
            .collect();
        let numbered = !marks.contains(&Some(Mark::Word));
        let order = |i: usize| {
            let number = match parse(&units[i].stem).mark {
                Some(Mark::Number(n)) if numbered => n,
                _ => 0,
            };
            (number, units[i].newest())
        };
        members.sort_by_key(|&i| order(i));
        protos.push(Proto {
            base: parse(&units[members[0]].stem).base,
            members,
            guessed: true,
            name: None,
        });
    }
    protos
}

/// Дать имена. Занятое в проекте — не трогаем (файлы остаются); одинаковые
/// имена из разных папок различаем именем папки.
pub(super) fn name(
    protos: &mut [Proto],
    units: &[Unit],
    taken: &[PartName],
    left: &mut Vec<LeftFile>,
) {
    let mut used: Vec<PartName> = Vec::new();
    for proto in protos.iter_mut() {
        let files = || {
            proto
                .members
                .iter()
                .flat_map(|&m| units[m].models.iter().map(|f| f.path.clone()))
                .collect::<Vec<_>>()
        };
        let leave = |why: String, left: &mut Vec<LeftFile>| {
            for path in files() {
                left.push(LeftFile {
                    path,
                    why: why.clone(),
                });
            }
        };
        let Some(wanted) = PartName::cleaned(&proto.base) else {
            leave("из имени файла не получается имя детали".into(), left);
            continue;
        };
        if taken.iter().any(|t| t.same_as(&wanted)) {
            leave(format!("в проекте уже есть «{wanted}»"), left);
            continue;
        }
        let dir = &units[proto.members[0]].dir;
        let folder = dir.rsplit('/').next().unwrap_or_default();
        let free = |n: &PartName| !used.iter().chain(taken).any(|u| u.same_as(n));
        let found = std::iter::once(Some(wanted.clone()))
            .chain(std::iter::once(
                (!folder.is_empty())
                    .then(|| PartName::cleaned(&format!("{wanted} ({folder})")))
                    .flatten(),
            ))
            .chain((2..100).map(|n| PartName::cleaned(&format!("{wanted} ({n})"))))
            .flatten()
            .find(|n| free(n));
        match found {
            Some(n) => {
                used.push(n.clone());
                proto.name = Some(n);
            }
            None => leave("не подобрал свободное имя".into(), left),
        }
    }
}
