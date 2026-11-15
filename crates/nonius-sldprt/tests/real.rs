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
