//! Замер сравнения на своих деталях: сколько идёт каждый шаг и что
//! получается. Запуск:
//!
//! ```sh
//! cargo run --release -p nonius-occt --example probe -- было.step стало.step
//! ```
//!
//! Первый запуск читает STEP, следующие берут тела из кэша.

#[cfg(not(nonius_occt))]
fn main() {
    eprintln!("Программа собрана без OpenCascade: мерить нечего.");
}

#[cfg(nonius_occt)]
fn main() {
    use std::path::PathBuf;
    use std::time::Instant;

    use nonius_occt::{changes, GeometryDiff, Shape};

    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [before, after] = args.as_slice() else {
        eprintln!("Нужны два файла: было.step стало.step");
        std::process::exit(2);
    };
    let mut start = Instant::now();
    let mut lap = |what: &str| {
        println!("{:>8.2} с  {what}", start.elapsed().as_secs_f64());
        start = Instant::now();
    };

    let was = Shape::read_step(before).expect("было");
    lap("чтение «было»");
    let now = Shape::read_step(after).expect("стало");
    lap("чтение «стало»");

    match was.changed_faces(&now) {
        Some(changed) => {
            lap("сопоставление граней");
            let gone = changed
                .faces
                .iter()
                .filter(|f| f.side == changes::Side::Before)
                .count();
            println!(
                "            граней пропало {gone}, появилось {}",
                changed.faces.len() - gone
            );
            for line in changes::describe(&changed.faces, 0.01) {
                println!("            • {line}");
            }
            let _ = changed.fresh.mesh(0.1, 0.1);
            let _ = changed.gone.mesh(0.1, 0.1);
            lap("сетка изменённых граней");
        }
        None => lap("грани не сопоставились"),
    }

    let diff = GeometryDiff::of_shapes(was, now, 0.01).expect("сравнение");
    lap("булева разница");
    println!(
        "            добавлено {:.1} мм³ (кусков {}), убрано {:.1} мм³ (кусков {})",
        diff.added_volume, diff.added_pieces, diff.removed_volume, diff.removed_pieces
    );
    let scene = diff.scene();
    lap("сцена");
    println!("            мест в сцене: {}", scene.changes().count());
}
