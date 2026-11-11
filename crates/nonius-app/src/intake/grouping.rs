//! Какие модели — одна деталь, и как её назвать.
//!
//! `вал`, `вал_v2`, `вал (3)` — версии одной детали; но файлы, которые
//! вместе стоят в одной сборке, — разные детали, как бы они ни назывались.
//! Одно имя в разных папках — копии проекта: одинаковые файлы склеиваются
//! в одну деталь, отличающиеся проверяет форма (`split.rs`, `shape.rs`).

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::SystemTime;

use nonius_core::PartName;

use super::names::{parse, Mark};
use super::shape::{Shape, ShapeVerdict};
use super::split::{split, Cluster, Evidence};
use super::units::Unit;
use super::LeftFile;

/// Будущая деталь: одна модель или несколько версий одной.
pub(super) struct Proto {
    /// Версии от старой к новой.
    pub members: Vec<usize>,
    /// Копии версий из других папок: в проект не идут, но их чертежи и
    /// сборки, где они стоят, относятся к этой детали.
    pub copies: Vec<usize>,
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

impl Proto {
    /// Все модели детали: версии и их копии.
    pub fn all(&self) -> impl Iterator<Item = usize> + '_ {
        self.members.iter().chain(&self.copies).copied()
    }
    /// Последняя версия: по ней деталь получает папку в проекте.
    pub fn home(&self) -> usize {
        self.members[self.members.len() - 1]
    }
}

/// Папка словами.
fn place(dir: &str) -> String {
    if dir.is_empty() {
        "корень".to_string()
    } else {
        format!("папка «{dir}»")
    }
}

/// Имя модели для человека. Если в `group` есть тёзка из другой папки —
/// с папкой, иначе их не различить.
pub(super) fn label(units: &[Unit], group: &[usize], i: usize) -> String {
    let stem = &units[i].stem;
    let namesake = |o: &usize| *o != i && units[*o].stem.to_lowercase() == stem.to_lowercase();
    if group.iter().any(namesake) {
        format!("{stem} ({})", place(&units[i].dir))
    } else {
        stem.clone()
    }
}

/// Собрать версии одной детали. В одну группу идут модели с одним именем
/// без пометок версий — `вал`, `вал_v2`, `вал (3)` — из всех папок: проект
/// часто копируют папкой целиком («рабочая версия», «готовая»). Дальше:
/// - побайтно одинаковые файлы — одна и та же деталь, копии в проект не
///   идут (остаётся та, что в самой «живой» папке);
/// - остальное делит `split.rs`: сборка, слово человека, форма, имя.
///
/// `across` — верить одному имени в разных папках без формы. Так строится
/// черновик плана, по которому видно, чью форму смотреть; в готовом плане
/// без формы одинаковые имена из разных папок остаются разными деталями.
pub(super) fn group(
    units: &[Unit],
    used_by: &[BTreeSet<usize>],
    shape: &dyn Fn(usize) -> Option<Shape>,
    decided: &dyn Fn(usize, usize) -> Option<bool>,
    across: bool,
) -> Vec<Proto> {
    let mut by_base: BTreeMap<(bool, String), Vec<usize>> = BTreeMap::new();
    // «Живая» папка — та, где файлы меняли позже всех.
    let mut fresh: HashMap<&str, SystemTime> = HashMap::new();
    for (i, u) in units.iter().enumerate() {
        let base = parse(&u.stem).base.to_lowercase();
        by_base.entry((u.is_assembly(), base)).or_default().push(i);
        let at = fresh.entry(&u.dir).or_insert(SystemTime::UNIX_EPOCH);
        *at = (*at).max(u.newest());
    }
    let rank = |i: usize| {
        let dir = units[i].dir.as_str();
        (fresh.get(dir).copied(), Reverse(dir.len()), Reverse(dir))
    };
    let same_file = |a: usize, b: usize| {
        units[a].content().is_some() && units[a].content() == units[b].content()
    };
    let order = |members: &[usize], i: usize| {
        let marks = members.iter().map(|&m| parse(&units[m].stem).mark);
        let numbered = !marks.into_iter().any(|m| m == Some(Mark::Word));
        let number = match parse(&units[i].stem).mark {
            Some(Mark::Number(n)) if numbered => n,
            _ => 0,
        };
        (number, units[i].newest())
    };
    let mut protos = Vec::new();
    for members in by_base.into_values() {
        // Одинаковые файлы: из каждой стопки копий остаётся один.
        let mut keepers: Vec<usize> = Vec::new();
        let mut twins: Vec<(usize, usize)> = Vec::new();
        for &m in &members {
            match keepers.iter().position(|&k| same_file(k, m)) {
                Some(p) if rank(m) > rank(keepers[p]) => {
                    let old = std::mem::replace(&mut keepers[p], m);
                    twins
                        .iter_mut()
                        .filter(|t| t.0 == old)
                        .for_each(|t| t.0 = m);
                    twins.push((m, old));
                }
                Some(p) => twins.push((keepers[p], m)),
                None => keepers.push(m),
            }
        }
        let sorted = keepers.clone();
        keepers.sort_by_key(|&i| order(&sorted, i));
        let name_of = |i: usize| label(units, &members, i);
        let evidence = Evidence {
            shape,
            label: &name_of,
            decided,
            apart: &|a, b| used_by[a].intersection(&used_by[b]).next().is_some(),
            trust: &|a, b| across || units[a].dir == units[b].dir,
            twin: &|a, b| {
                units[a].dir != units[b].dir
                    && units[a].stem.to_lowercase() == units[b].stem.to_lowercase()
            },
        };
        let parts = if keepers.len() > 1 {
            split(&keepers, &evidence)
        } else {
            vec![Cluster {
                members: keepers.clone(),
                dupes: Vec::new(),
                verdict: None,
                notes: Vec::new(),
                rival: None,
                settled: false,
            }]
        };
        for part in parts {
            let mut copies = part.dupes;
            let of_this = |k: usize| part.members.contains(&k) || copies.contains(&k);
            let files: Vec<usize> = twins.iter().filter(|t| of_this(t.0)).map(|t| t.1).collect();
            copies.extend(files);
            let mut notes = part.notes;
            if !copies.is_empty() {
                let mut dirs: Vec<String> = copies.iter().map(|&c| place(&units[c].dir)).collect();
                dirs.sort();
                dirs.dedup();
                notes.push(format!("та же деталь лежит ещё тут: {}", dirs.join(", ")));
            }
            // Одна версия — зовётся своим именем файла.
            let first = &units[part.members[0]].stem;
            let versions = part.members.len();
            protos.push(Proto {
                base: if versions == 1 {
                    first.clone()
                } else {
                    parse(first).base
                },
                members: part.members,
                copies,
                guessed: versions > 1 && !part.settled,
                name: None,
                shape: part.verdict,
                notes,
                rival: part.rival,
            });
        }
    }
    // По папкам и именам: от порядка зависит, кому достанется имя без
    // добавки, когда в разных папках лежат тёзки.
    protos.sort_by_key(|p| (units[p.home()].dir.clone(), p.base.to_lowercase()));
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
        kept.copies.extend(gone.copies);
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
        let dir = &units[proto.home()].dir;
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
