//! Правила разбора папки: из списка файлов и связей между ними — план.
//!
//! Здесь нет ни диска, ни SolidWorks: только рассуждение. Поэтому правила
//! проверяются тестами на придуманных списках файлов.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use nonius_core::meta::ItemKind;
use nonius_core::PartName;

use super::grouping::{group, name};
use super::{key, Found, IntakePlan, LeftFile, PlannedItem, PlannedVersion};

/// Файлы моделей: из них получаются детали и сборки.
const MODELS: &[&str] = &["sldprt", "sldasm", "step", "stp"];

/// Чертежи и бумаги конструктора: если такой файл ни к чему не подошёл,
/// человеку стоит о нём знать. Прочее (расчёты, логи, картинки программ) —
/// просто не наше.
const PAPERS: &[&str] = &[
    "slddrw", "dxf", "dwg", "pdf", "doc", "docx", "xls", "xlsx", "odt", "ods",
];

/// Модель одного имени в одной папке со всем, что к ней приложено.
pub(super) struct Unit {
    pub dir: String,
    pub stem: String,
    pub models: Vec<Found>,
    docs: Vec<PathBuf>,
    why: Vec<String>,
}

impl Unit {
    pub fn is_assembly(&self) -> bool {
        self.models.iter().any(|m| m.ext == "sldasm")
    }
    pub fn newest(&self) -> std::time::SystemTime {
        self.models
            .iter()
            .map(|m| m.modified)
            .max()
            .unwrap_or(std::time::UNIX_EPOCH)
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Составить план. `links` — связи файлов (ключи и значения — пути строчными
/// буквами, см. [`key`]); `taken` — имена, уже занятые в проекте.
pub(super) fn build(
    source: PathBuf,
    files: Vec<Found>,
    links: &HashMap<String, Vec<String>>,
    taken: &[PartName],
) -> IntakePlan {
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

    let mut protos = group(&units, &used_by);
    name(&mut protos, &units, taken, &mut left);

    // Сколько сборок в каждой папке: одна — папка и так её, несколько —
    // у каждой своя папка со своими деталями.
    let mut assemblies_in: HashMap<&str, usize> = HashMap::new();
    for p in protos.iter().filter(|p| p.name.is_some()) {
        if units[p.members[0]].is_assembly() {
            *assemblies_in.entry(&units[p.members[0]].dir).or_default() += 1;
        }
    }
    let owner: HashMap<usize, usize> = protos
        .iter()
        .enumerate()
        .flat_map(|(i, p)| p.members.iter().map(move |&m| (m, i)))
        .collect();

    let mut items = Vec::new();
    for proto in &protos {
        let Some(item_name) = &proto.name else {
            continue;
        };
        let first = &units[proto.members[0]];
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
                .members
                .iter()
                .flat_map(|&m| used_by[m].iter().copied())
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
            let labels: Vec<&str> = proto
                .members
                .iter()
                .map(|&m| units[m].stem.as_str())
                .collect();
            why.push(format!(
                "{} версии по именам файлов: {}",
                labels.len(),
                labels.join(" → ")
            ));
        }
        let mut docs: Vec<PathBuf> = Vec::new();
        for &m in &proto.members {
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
                    label: units[m].stem.clone(),
                    models: ordered(&units[m].models),
                })
                .collect(),
            docs,
            why,
            guessed: proto.guessed,
        });
    }
    items.sort_by(|a, b| (&a.folder, &a.name).cmp(&(&b.folder, &b.name)));
    left.sort_by(|a, b| a.path.cmp(&b.path));
    IntakePlan {
        source,
        items,
        left,
    }
}

type Index = HashMap<(String, String), usize>;

/// Модели по папке и имени: `вал.sldprt` и `вал.step` — одна модель.
fn units_of(files: &[Found]) -> (Vec<Unit>, Index, HashMap<String, usize>) {
    let mut units: Vec<Unit> = Vec::new();
    let mut by_name = Index::new();
    let mut by_path = HashMap::new();
    for f in files.iter().filter(|f| MODELS.contains(&f.ext.as_str())) {
        let id = (f.dir.clone(), f.stem.to_lowercase());
        let i = *by_name.entry(id).or_insert_with(|| {
            units.push(Unit {
                dir: f.dir.clone(),
                stem: f.stem.clone(),
                models: Vec::new(),
                docs: Vec::new(),
                why: Vec::new(),
            });
            units.len() - 1
        });
        units[i].models.push(f.clone());
        by_path.insert(key(&f.path), i);
    }
    (units, by_name, by_path)
}

/// Приложить чертежи и бумаги к моделям: по ссылке, потом по имени.
fn attach(
    files: &[Found],
    links: &HashMap<String, Vec<String>>,
    by_name: &Index,
    by_path: &HashMap<String, usize>,
    units: &mut [Unit],
    left: &mut Vec<LeftFile>,
) {
    for f in files.iter().filter(|f| !MODELS.contains(&f.ext.as_str())) {
        let linked = links
            .get(&key(&f.path))
            .into_iter()
            .flatten()
            .find_map(|target| by_path.get(target).copied());
        let named = by_name
            .get(&(f.dir.clone(), f.stem.to_lowercase()))
            .copied();
        match (linked, named) {
            (Some(u), _) => {
                if named != Some(u) {
                    units[u].why.push(format!(
                        "чертёж «{}» ссылается на эту модель",
                        file_name(&f.path)
                    ));
                }
                units[u].docs.push(f.path.clone());
            }
            (None, Some(u)) => units[u].docs.push(f.path.clone()),
            (None, None) => left.push(LeftFile {
                path: f.path.clone(),
                why: if f.ext == "slddrw" {
                    "чертёж: не нашёл его модель в этой папке".into()
                } else if PAPERS.contains(&f.ext.as_str()) {
                    "не понял, к какой детали относится".into()
                } else {
                    super::NOT_OURS.into()
                },
            }),
        }
    }
}

/// Файлы модели в порядке импорта: сначала STEP, потом файл SolidWorks —
/// свежий STEP из SolidWorks, если его удастся получить, главнее старого.
fn ordered(models: &[Found]) -> Vec<PathBuf> {
    let mut sorted: Vec<&Found> = models.iter().collect();
    sorted.sort_by_key(|m| (m.ext.starts_with("sld"), m.path.clone()));
    sorted.into_iter().map(|m| m.path.clone()).collect()
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
