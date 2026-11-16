//! Прогон по настоящим файлам — по желанию, через переменную окружения
//! `NONIUS_TEST_SLD_FOLDERS=папка;папка`. Файлы только читаются. В вывод
//! идут числа, а не имена файлов и не их содержимое.

use std::path::{Path, PathBuf};

use nonius_sldprt::{Container, Listing, Section};

/// Все файлы SolidWorks в папках из переменной окружения.
pub fn real_files() -> Vec<PathBuf> {
    let Some(folders) = std::env::var_os("NONIUS_TEST_SLD_FOLDERS") else {
        return Vec::new();
    };
    let mut files = Vec::new();
    let mut queue: Vec<PathBuf> = std::env::split_paths(&folders).collect();
    while let Some(folder) = queue.pop() {
        let Ok(list) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in list.flatten() {
            let path = entry.path();
            if path.is_dir() {
                queue.push(path);
            } else if is_solidworks(&path) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn is_solidworks(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    // `~$…` — файлы-замки открытых документов, по 5 байт.
    !name.starts_with("~$")
        && ["sldprt", "sldasm", "slddrw"]
            .iter()
            .any(|e| ext.eq_ignore_ascii_case(e))
}

/// Каждый настоящий файл открывается по каталогу, и каждая его секция
/// распаковывается с верным размером и контрольной суммой.
#[test]
fn real_containers_are_read_whole() {
    let files = real_files();
    if files.is_empty() {
        return;
    }
    let started = std::time::Instant::now();
    let (mut sections, mut bytes, mut bad) = (0usize, 0u64, 0usize);
    for path in &files {
        let file = match Container::open(path) {
            Ok(file) => file,
            Err(why) => {
                bad += 1;
                println!("не открылся файл №{bad}: {why}");
                continue;
            }
        };
        assert_eq!(file.listing(), Listing::Catalog, "каталог не найден");
        for name in file.names() {
            sections += 1;
            assert!(
                matches!(file.section(&name), Section::Data(_)),
                "секция не читается"
            );
        }
        bytes += file.unpacked();
    }
    println!(
        "файлов {}, не открылось {bad}, секций {sections}, распаковано {} МБ; {:.1?}",
        files.len(),
        bytes >> 20,
        started.elapsed()
    );
    assert_eq!(bad, 0);
}

/// Замысел читается из каждого настоящего файла; в вывод — счёт по кодам
/// пропусков и признаков спорности.
#[test]
fn real_designs_are_read() {
    use nonius_sldprt::design::DocKind;
    use nonius_sldprt::Design;
    use std::collections::BTreeMap;

    let files = real_files();
    if files.is_empty() {
        return;
    }
    let started = std::time::Instant::now();
    let mut count: BTreeMap<String, usize> = BTreeMap::new();
    let mut add = |key: String| *count.entry(key).or_default() += 1;
    for path in &files {
        let design = Design::open(path).expect("файл открывается");
        add(format!("вид: {:?}", design.kind));
        if design.gaps.is_empty() {
            add("прочитано целиком".into());
        }
        for gap in &design.gaps {
            add(format!("пропуск: {}", gap.code()));
        }
        for doubt in design.doubts() {
            add(format!("спорно: {}", doubt.code()));
        }
        let tree = design
            .tree
            .as_ref()
            .expect("список операций есть в каждом файле");
        add(format!(
            "операций: {}",
            if tree.operations.is_empty() {
                "нет"
            } else {
                "есть"
            }
        ));
        if tree.skipped > 0 {
            add("есть узлы без номера".into());
        }
        if design.kind == Some(DocKind::Part) {
            let manifest = design.manifest.as_ref().expect("манифест детали");
            // Номер корня списка операций — время создания файла.
            assert_eq!(tree.root_id, manifest.documents[0].created);
            assert!(design.active_config().is_some());
            if design.material().is_some() {
                add("материал назван".into());
            }
            if design.saved_mass().is_some() {
                add("масса сохранена".into());
            }
        }
        if design.kind == Some(DocKind::Assembly) {
            let manifest = design.manifest.as_ref().expect("манифест сборки");
            add(format!(
                "сборка: компонентов {}",
                if manifest.components.is_empty() {
                    "нет"
                } else {
                    "есть"
                }
            ));
        }
        if design.kind == Some(DocKind::Drawing) && !design.props.drawing_models.is_empty() {
            add("чертёж: модель названа".into());
        }
    }
    for (key, n) in &count {
        println!("{n:>5}  {key}");
    }
    println!("файлов {}; {:.1?}", files.len(), started.elapsed());
}

/// Замысел одного файла (`NONIUS_TEST_SLDPRT=путь`): числа и коды.
#[test]
fn real_file_design_summary() {
    let Some(file) = std::env::var_os("NONIUS_TEST_SLDPRT") else {
        return;
    };
    let design = nonius_sldprt::Design::open(Path::new(&file)).unwrap();
    let tree = design.tree.as_ref().unwrap();
    let real = tree.operations.iter().filter(|o| !o.service).count();
    println!(
        "операций {} (не служебных {real}), пропуски {:?}, спорно {:?}",
        tree.operations.len(),
        design.gaps.iter().map(|g| g.code()).collect::<Vec<_>>(),
        design.doubts()
    );
}

/// Сравнение двух настоящих версий (`NONIUS_TEST_SLDPRT_PAIR=старая;новая`):
/// что добавлено, удалено, переименовано, какой размер изменился.
#[test]
fn real_pair_is_compared() {
    use nonius_core::design::{diff, Change};
    use nonius_sldprt::{to_core, Design};

    let Some(pair) = std::env::var_os("NONIUS_TEST_SLDPRT_PAIR") else {
        return;
    };
    let files: Vec<PathBuf> = std::env::split_paths(&pair).collect();
    let read = |path: &Path| to_core(&Design::open(path).unwrap());
    let (before, after) = (read(&files[0]), read(&files[1]));
    assert!(before.is_whole() && after.is_whole());
    let changes = diff(&before.value, &after.value);
    for change in changes.iter().filter(|c| !c.is_service()) {
        match change {
            Change::Added { id, name, kind, .. } => println!("добавлено: {name} ({kind}, id {id})"),
            Change::Removed { id, name, kind, .. } => println!("удалено: {name} ({kind}, id {id})"),
            Change::Renamed {
                id, before, after, ..
            } => {
                println!("переименовано: {before} -> {after} (id {id})")
            }
            Change::Resized {
                operation,
                size,
                before,
                after,
                ..
            } => {
                let shown = |s: &Option<nonius_core::design::Size>| {
                    s.as_ref().map_or("нет".to_string(), |s| s.shown.clone())
                };
                println!(
                    "размер: {operation} {size}: {} -> {}",
                    shown(before),
                    shown(after)
                );
            }
        }
    }
    let service = changes.iter().filter(|c| c.is_service()).count();
    println!("изменений {}, из них служебных {service}", changes.len());
    assert!(diff(&before.value, &before.value).is_empty());
}
