//! Форма из файла `.sldprt`: файл собирается прямо в тесте.
//!
//! Настоящие файлы — по желанию, через переменные окружения:
//! - `NONIUS_TEST_SLDPRT=путь.sldprt` — контракт на настоящем файле;
//! - `NONIUS_TEST_SLDPRT_TRUTH=shapes.tsv` — сверка с эталоном: строки
//!   `объём⇥площадь⇥габарит×3⇥грани⇥путь*размер*время|…` (так пишет память
//!   разбора папки, `intake-shapes.tsv`, когда форму снял SolidWorks).

use std::path::{Path, PathBuf};

use nonius_core::contract;
use nonius_core::extension::ShapeGlance;
use nonius_sldprt::{fingerprint, preview_png, Container, SldprtGlance};

type V = [f32; 3];

/// Запись одной грани: полосы вершин (в метрах) с нормалями.
fn face(strips: &[Vec<(V, V)>]) -> Vec<u8> {
    let number = |n: usize| (n as u32).to_le_bytes();
    let total: usize = strips.iter().map(Vec::len).sum();
    let mut out = vec![0xAA; 7]; // перед записью в файле лежит разное
    out.extend([4, 0, 0, 0, 8, 0, 0, 0, 2, 0, 0, 0]);
    out.extend(number(strips.len()));
    for strip in strips {
        out.extend(number(strip.len()));
    }
    for pick in [0, 1] {
        out.extend([12, 0, 0, 0, 100, 0, 0, 0, 2, 0, 0, 0]);
        out.extend(number(total));
        for (point, normal) in strips.iter().flatten() {
            let v = if pick == 0 { point } else { normal };
            out.extend(v.iter().flat_map(|x| x.to_le_bytes()));
        }
    }
    out
}

/// Плоская грань-прямоугольник: угол `o`, стороны `u` и `v`, нормаль `n`.
fn quad(o: V, u: V, v: V, n: V) -> Vec<u8> {
    let at = |a: f32, b: f32| -> V { std::array::from_fn(|k| o[k] + a * u[k] + b * v[k]) };
    let strip = [at(0., 0.), at(1., 0.), at(0., 1.), at(1., 1.)];
    face(&[strip.iter().map(|p| (*p, n)).collect()])
}

/// Брусок 10×20×30 мм.
fn block() -> Vec<u8> {
    let (x, y, z) = ([0.01, 0., 0.], [0., 0.02, 0.], [0., 0., 0.03]);
    let o = [0.; 3];
    [
        quad(o, x, y, [0., 0., -1.]),
        quad(z, x, y, [0., 0., 1.]),
        quad(o, x, z, [0., -1., 0.]),
        quad(y, x, z, [0., 1., 0.]),
        quad(o, y, z, [-1., 0., 0.]),
        quad(x, y, z, [1., 0., 0.]),
    ]
    .concat()
}

/// Цилиндр ⌀20×20 мм, нарисованный грубо: 12 граней по кругу.
fn cylinder() -> Vec<u8> {
    const N: usize = 12;
    let (r, h) = (0.01_f32, 0.02_f32);
    let dir = |i: usize| {
        let a = std::f32::consts::TAU * (i % N) as f32 / N as f32;
        [a.cos(), a.sin()]
    };
    let rim = |i: usize, z: f32| -> V { [r * dir(i)[0], r * dir(i)[1], z] };
    let side = (0..=N)
        .flat_map(|i| {
            let n = [dir(i)[0], dir(i)[1], 0.];
            [(rim(i, 0.), n), (rim(i, h), n)]
        })
        .collect();
    // Крышка одной лентой: вершины берём по очереди с двух сторон круга.
    let cap = |z: f32, n: V| -> Vec<(V, V)> {
        let order = (0..N).map(|k| if k % 2 == 0 { k / 2 } else { N - 1 - k / 2 });
        order.map(|i| (rim(i, z), n)).collect()
    };
    [
        face(&[side]),
        face(&[cap(0., [0., 0., -1.])]),
        face(&[cap(h, [0., 0., 1.])]),
    ]
    .concat()
}

/// Файл детали с сеткой `lists`; метки сетки и модели — `mesh` и `model`.
fn part(dir: &Path, lists: Option<&[u8]>, mesh: u32, model: u32) -> PathBuf {
    let stamps = [[0; 4], model.to_le_bytes(), [0; 4]].concat();
    let stamp = [mesh.to_le_bytes().as_slice(), &[0, 0]].concat();
    let mut chunks: Vec<(&str, &[u8])> = vec![
        ("ModelStamps", &stamps),
        ("_DL_VERSION_16000/DLUpdateStamp", &stamp),
        ("PreviewPNG", b"\x89PNG\r\n\x1a\npicture"),
    ];
    if let Some(lists) = lists {
        chunks.push(("Contents/DisplayLists", lists));
    }
    let path = dir.join("деталь.SLDPRT");
    std::fs::write(&path, Container::pack(&chunks)).unwrap();
    path
}

fn close(a: f64, b: f64, share: f64) -> bool {
    (a - b).abs() <= share * b.abs()
}

#[test]
fn block_is_measured_and_passes_contract() {
    let tmp = tempfile::tempdir().unwrap();
    let file = part(tmp.path(), Some(&block()), 7, 7);
    let shape = fingerprint(&file).unwrap();
    assert!(close(shape.volume, 6000.0, 1e-5), "{shape:?}");
    assert!(close(shape.area, 2200.0, 1e-5), "{shape:?}");
    assert_eq!(shape.faces, 6);
    let size: Vec<f64> = (0..3).map(|k| shape.max[k] - shape.min[k]).collect();
    assert!(close(size[0], 10.0, 1e-5) && close(size[1], 20.0, 1e-5) && close(size[2], 30.0, 1e-5));

    if let Err(v) = contract::glance(&SldprtGlance::new(), &file) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
    assert!(!SldprtGlance::new().can_glance(Path::new("сборка.sldasm")));
}

#[test]
fn round_part_volume_is_corrected_by_normals() {
    let tmp = tempfile::tempdir().unwrap();
    let shape = fingerprint(&part(tmp.path(), Some(&cylinder()), 1, 1)).unwrap();
    let exact = std::f64::consts::PI * 10.0 * 10.0 * 20.0;
    // Двенадцатигранник меньше цилиндра на 4,5 %; с поправкой — доли процента.
    assert!(
        close(shape.volume, exact, 0.005),
        "{} и {exact}",
        shape.volume
    );
    assert_eq!(shape.faces, 3);
}

#[test]
fn refuses_instead_of_guessing() {
    let tmp = tempfile::tempdir().unwrap();
    let stale = fingerprint(&part(tmp.path(), Some(&block()), 7, 8)).unwrap_err();
    assert!(stale.contains("старее"), "{stale}");
    let bare = fingerprint(&part(tmp.path(), None, 7, 7)).unwrap_err();
    assert!(bare.contains("нет сетки"), "{bare}");
    // Без одной стенки сетка не замкнута.
    let open = &block()[..block().len() / 6 * 5];
    let open = fingerprint(&part(tmp.path(), Some(open), 7, 7)).unwrap_err();
    assert!(open.contains("не замкнута"), "{open}");
    // Не файл SolidWorks и обрезанный файл — отказ, а не падение.
    let junk = tmp.path().join("мусор.sldprt");
    std::fs::write(&junk, b"\x14\x00\x06\x00\x08\x00 not a part").unwrap();
    assert!(fingerprint(&junk).is_err());
    let whole = std::fs::read(part(tmp.path(), Some(&block()), 7, 7)).unwrap();
    std::fs::write(&junk, &whole[..whole.len() - 40]).unwrap();
    assert!(fingerprint(&junk).is_err());
    assert!(SldprtGlance::new().glance(&junk).is_err());
}

#[test]
fn preview_is_taken_from_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let file = part(tmp.path(), None, 1, 1);
    assert!(preview_png(&file).unwrap().starts_with(b"\x89PNG"));
    assert_eq!(preview_png(&tmp.path().join("нет.sldprt")), None);
    let names = Container::open(&file).unwrap().names();
    assert!(names.contains(&"PreviewPNG".to_string()), "{names:?}");
}

#[test]
fn real_file_passes_contract() {
    let Some(file) = std::env::var_os("NONIUS_TEST_SLDPRT") else {
        return;
    };
    let file = PathBuf::from(file);
    if let Err(v) = contract::glance(&SldprtGlance::new(), &file) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
    assert!(preview_png(&file).is_some(), "в файле нет превью");
    println!("{:?}", fingerprint(&file).unwrap());
}

/// Сверка с эталоном: габариты в пределах 1 %, объём — 2 %.
#[test]
fn real_files_match_the_truth() {
    let Some(truth) = std::env::var_os("NONIUS_TEST_SLDPRT_TRUTH") else {
        return;
    };
    let text = std::fs::read_to_string(truth).unwrap();
    let (mut all, mut matched, mut refused, mut wrong) = (0, 0, 0, 0);
    let started = std::time::Instant::now();
    for line in text.lines() {
        let cells: Vec<&str> = line.split('\t').collect();
        let number = |i: usize| cells[i].parse::<f64>().unwrap();
        let files = cells[6].split('|').map(|s| s.split('*').next().unwrap());
        let Some(file) = files.into_iter().find(|f| f.ends_with(".sldprt")) else {
            continue;
        };
        all += 1;
        match fingerprint(Path::new(file)) {
            Err(why) => {
                refused += 1;
                println!("отказ: {file}: {why}");
            }
            Ok(shape) => {
                let mut size: Vec<f64> = (0..3).map(|k| shape.max[k] - shape.min[k]).collect();
                size.sort_by(f64::total_cmp);
                let sizes = (0..3).all(|k| close(size[k], number(2 + k), 0.01));
                if sizes && close(shape.volume, number(0), 0.02) {
                    matched += 1;
                } else {
                    wrong += 1;
                    println!(
                        "не сошлось: {file}: объём {:.0} (эталон {:.0}), габариты {size:.1?} \
                         (эталон {:.1} {:.1} {:.1})",
                        shape.volume,
                        number(0),
                        number(2),
                        number(3),
                        number(4)
                    );
                }
            }
        }
    }
    println!(
        "деталей {all}: сошлось {matched}, отказ {refused}, не сошлось {wrong}; {:.1?}",
        started.elapsed()
    );
    assert!(matched * 10 >= all * 8, "сошлось меньше 80 %");
}
