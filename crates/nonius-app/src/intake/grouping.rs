//! Какие модели — одна деталь, и как её назвать.
//!
//! `вал`, `вал_v2`, `вал (3)` — версии одной детали; но файлы, которые
//! вместе стоят в одной сборке, — разные детали, как бы они ни назывались.
//! Догадку по именам проверяет форма (`shape.rs`): непохожие расходятся.

use std::collections::{BTreeMap, BTreeSet};

use nonius_core::PartName;

use super::names::{parse, Mark};
use super::shape::{split, Shape, ShapeVerdict};
use super::units::Unit;
use super::LeftFile;

/// Будущая деталь: одна модель или несколько версий одной.
pub(super) struct Proto {
    pub members: Vec<usize>,
    pub base: String,
    pub guessed: bool,
    pub name: Option<PartName>,
    /// Что сказала о версиях форма; `None` — не проверялось.
    pub shape: Option<ShapeVerdict>,
    /// Строки «почему» от проверки формой и от ответов человека.
    pub notes: Vec<String>,
    /// Модель, от которой эту отделила форма: о паре стоит спросить.
    pub rival: Option<usize>,
}

/// Собрать версии одной детали по именам: `вал`, `вал_v2`, `вал (3)`.
/// Файлы, которые вместе стоят в одной сборке, — разные детали. `shape` —
/// отпечаток формы модели по её номеру: группу с отпечатками проверяет
/// форма, и непохожие становятся отдельными деталями.
pub(super) fn group(
    units: &[Unit],
    used_by: &[BTreeSet<usize>],
    shape: &dyn Fn(usize) -> Option<Shape>,
    decided: &dyn Fn(usize, usize) -> Option<bool>,
) -> Vec<Proto> {
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
        shape: None,
        notes: Vec::new(),
        rival: None,
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
        for part in split(&members, shape, &|i| units[i].stem.clone(), decided) {
            // Отделилась от группы — зовётся своим именем файла.
            let alone = part.members.len() == 1;
            let first = &units[part.members[0]].stem;
            protos.push(Proto {
                base: if alone {
                    first.clone()
                } else {
                    parse(first).base
                },
                members: part.members,
                guessed: !alone && !part.settled,
                name: None,
                shape: part.verdict,
                notes: part.notes,
                rival: part.rival,
            });
        }
    }
    protos
}

/// Соединить детали с разными именами, про которые человек сказал «это
/// версии одной детали». Версии идут по времени файла; имя — от новой.
pub(super) fn merge(
    protos: &mut Vec<Proto>,
    units: &[Unit],
    decided: &dyn Fn(usize, usize) -> Option<bool>,
) {
    loop {
        let same = |a: &Proto, b: &Proto| {
            units[a.members[0]].is_assembly() == units[b.members[0]].is_assembly()
                && a.members
                    .iter()
                    .any(|&m| b.members.iter().any(|&n| decided(m, n) == Some(true)))
        };
        let found = (0..protos.len())
            .flat_map(|a| (a + 1..protos.len()).map(move |b| (a, b)))
            .find(|&(a, b)| same(&protos[a], &protos[b]));
        let Some((a, b)) = found else {
            break;
        };
        let gone = protos.remove(b);
        let kept = &mut protos[a];
        kept.members.extend(gone.members);
        kept.members.sort_by_key(|&m| units[m].newest());
        if let Some(&newest) = kept.members.last() {
            kept.base = parse(&units[newest].stem).base;
        }
        kept.guessed = false;
        kept.shape = None;
        kept.rival = None;
        kept.notes = vec!["вы ответили: это версии одной детали".into()];
    }
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
