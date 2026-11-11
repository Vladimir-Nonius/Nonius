//! Правила разбора папки: из списка файлов и связей между ними — план.
//!
//! Здесь нет ни диска, ни SolidWorks: только рассуждение. Поэтому правила
//! проверяются тестами на придуманных списках файлов.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use nonius_core::meta::ItemKind;
use nonius_core::PartName;

use super::grouping::{group, label, merge, name};
use super::hints::hints;
use super::questions::{ask, Decisions};
use super::shape::Shape;
use super::units::{attach, file_name, ordered, units_of};
use super::words::Words;
use super::{key, Found, IntakePlan, PlannedItem, PlannedVersion};

/// Что известно о файлах папки, кроме их списка.
#[derive(Clone, Copy)]
pub(super) struct Facts<'a> {
    /// Связи файлов (ключи и значения — пути строчными буквами, см. [`key`]).
    pub links: &'a HashMap<String, Vec<String>>,
    /// Имена, уже занятые в проекте.
    pub taken: &'a [PartName],
    /// Отпечатки формы моделей по ключу файла (каких нет — те не
    /// проверяются).
    pub shapes: &'a HashMap<String, Shape>,
    /// Ответы человека на вопросы прошлого плана.
    pub said: &'a Decisions,
    /// Черновик: верить одному имени в разных папках, пока форму не
    /// смотрели.
    pub across: bool,
    /// Что изменилось между версиями словами (чего там нет, то запомнится
    /// как недостающее).
    pub words: &'a Words,
}

/// Составить план из списка файлов и того, что о них известно.
pub(super) fn build(source: PathBuf, files: Vec<Found>, facts: Facts) -> IntakePlan {
    let Facts {
        links,
        taken,
        shapes,
        said,
        across,
        words,
    } = facts;
    let mut left = Vec::new();
    let (mut units, by_name, by_path) = units_of(&files);
    attach(&files, links, &by_name, &by_path, &mut units, &mut left);

    // В каких сборках стоит каждая модель.
    let mut used_by: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); units.len()];
    for (a, unit) in units.iter().enumerate() {
        for model in unit.models.iter().filter(|m| m.ext == "sldasm") {
            for target in links.get(&key(&model.path)).into_iter().flatten() {
                if let Some(&u) = by_path.get(target) {
                    if u != a {
                        used_by[u].insert(a);
                    }
                }
            }
        }
    }

    let shape = |u: usize| -> Option<Shape> {
        let models = units[u].models.iter();
        models
            .filter_map(|m| shapes.get(&key(&m.path)))
            .next()
            .copied()
    };
    let decided = |a: usize, b: usize| said.get(&units[a].id(), &units[b].id());
    let mut protos = group(&units, &used_by, &shape, &decided, across);
    merge(&mut protos, &units, &decided);
    name(&mut protos, &units, taken, &mut left);

    // Похожие формой детали с непохожими именами: только подсказка.
    let parts: Vec<(String, Option<Shape>)> = protos
        .iter()
        .map(|p| {
            let last = p
                .members
                .last()
                .copied()
                .filter(|&m| !units[m].is_assembly());
            let label = p.name.as_ref().map(ToString::to_string);
            (
                label.clone().unwrap_or_default(),
                label.and(last).and_then(shape),
            )
        })
        .collect();
    let users = |p: usize| -> BTreeSet<usize> {
        let members = protos[p].all();
        members.flat_map(|m| used_by[m].iter().copied()).collect()
    };
    let refused = |a: usize, b: usize| {
        let (x, y) = (&protos[a].members, &protos[b].members);
        x.iter()
            .any(|&m| y.iter().any(|&n| decided(m, n) == Some(false)))
    };
    let apart =
        |a: usize, b: usize| refused(a, b) || users(a).intersection(&users(b)).next().is_some();
    let similar = hints(&parts, &apart);
    // Что изменилось между двумя моделями — словами; про сборки не знаем.
    let changed = |a: usize, b: usize| -> Vec<String> {
        if units[a].is_assembly() || units[b].is_assembly() {
            return Vec::new();
        }
        words.get(&ordered(&units[a].models), &ordered(&units[b].models))
    };
    let questions = ask(&protos, &units, &similar.alike, &changed);

    // Сколько сборок в каждой папке: одна — папка и так её, несколько —
    // у каждой своя папка со своими деталями.
    let mut assemblies_in: HashMap<&str, usize> = HashMap::new();
    for p in protos.iter().filter(|p| p.name.is_some()) {
        if units[p.home()].is_assembly() {
            *assemblies_in.entry(&units[p.home()].dir).or_default() += 1;
        }
    }
    let owner: HashMap<usize, usize> = protos
        .iter()
        .enumerate()
        .flat_map(|(i, p)| p.all().map(move |m| (m, i)))
        .collect();

    let mut items = Vec::new();
    for (n, proto) in protos.iter().enumerate() {
        let Some(item_name) = &proto.name else {
            continue;
        };
        let first = &units[proto.home()];
        let assembly = first.is_assembly();
        let mut why: Vec<String> = Vec::new();
        let mut folder = first.dir.clone();
        let into = |folder: &mut String, name: &str| {
            if !folder.is_empty() {
                folder.push('/');
            }
            folder.push_str(name);
        };
        if assemblies_in.get(first.dir.as_str()).copied().unwrap_or(0) >= 2 {
            let users: BTreeSet<usize> = proto
                .all()
                .flat_map(|m| used_by[m].iter().copied())
                .filter(|&a| units[a].dir == first.dir)
                .filter_map(|a| owner.get(&a).copied())
                .collect();
            if assembly {
                into(&mut folder, item_name.as_str());
            } else if users.len() == 1 {
                let user = &protos[*users.iter().next().unwrap_or(&0)];
                if let Some(user_name) = &user.name {
                    into(&mut folder, user_name.as_str());
                    why.push(format!("нужна только сборке «{user_name}»"));
                }
            } else if users.len() > 1 {
                why.push(format!("стоит в {} сборках — лежит отдельно", users.len()));
            }
        }
        if proto.members.len() > 1 {
            let labels: Vec<String> = proto
                .members
                .iter()
                .map(|&m| label(&units, &proto.members, m))
                .collect();
            why.push(format!(
                "{} версии по именам файлов: {}",
                labels.len(),
                labels.join(" → ")
            ));
        }
        why.extend(proto.notes.iter().cloned());
        for pair in proto.members.windows(2) {
            let lines = changed(pair[0], pair[1]);
            if !lines.is_empty() {
                why.push(format!(
                    "что изменилось, «{}» → «{}»: {}",
                    label(&units, &proto.members, pair[0]),
                    label(&units, &proto.members, pair[1]),
                    lines.join("; ")
                ));
            }
        }
        why.extend(similar.notes[n].iter().cloned());
        let mut docs: Vec<PathBuf> = Vec::new();
        for m in proto.all() {
            why.extend(units[m].why.iter().cloned());
            for doc in &units[m].docs {
                // В `docs/` детали имена не повторяются: берём первый.
                if !docs.iter().any(|d| file_name(d) == file_name(doc)) {
                    docs.push(doc.clone());
                }
            }
        }
        items.push(PlannedItem {
            name: item_name.to_string(),
            kind: if assembly {
                ItemKind::Assembly
            } else {
                ItemKind::Part
            },
            folder,
            versions: proto
                .members
                .iter()
                .map(|&m| PlannedVersion {
                    label: label(&units, &proto.members, m),
                    models: ordered(&units[m].models),
                })
                .collect(),
            docs,
            why,
            guessed: proto.guessed,
            shape: proto.shape,
        });
    }
    items.sort_by(|a, b| (&a.folder, &a.name).cmp(&(&b.folder, &b.name)));
    left.sort_by(|a, b| a.path.cmp(&b.path));
    IntakePlan {
        source,
        items,
        left,
        questions,
    }
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
