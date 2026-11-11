//! Изменения словами инженера: «отверстие ⌀8 → ⌀10», «плоскость сдвинута».
//!
//! Булева разница отвечает, сколько материала добавлено и убрано. Здесь —
//! что сделали: грани, которые есть только в одной из версий ([`Face`]),
//! складываются в отверстия, бобышки, скругления и плоскости, а пары
//! «было — стало» описываются словами. OpenCascade тут не нужен: грани уже
//! описаны числами, поэтому логика проверяется тестами без него.
//!
//! Файлы: здесь — грань и сбор отчёта, `round.rs` — цилиндры (отверстия,
//! бобышки, скругления), `flat.rs` — плоскости, `text.rs` — запись чисел.

mod flat;
mod round;
mod text;

use nonius_core::compare::{Change, ChangeKind};

/// В какой версии грань есть.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Была и пропала.
    Before,
    /// Появилась.
    After,
}

/// Вид поверхности грани.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Сплайн и всё остальное.
    Other,
    /// Плоскость.
    Plane,
    /// Цилиндр.
    Cylinder,
    /// Конус.
    Cone,
    /// Сфера.
    Sphere,
    /// Тор.
    Torus,
}

/// Грань, которая есть только в одной из версий. Единицы — мм и радианы.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Face {
    /// В какой версии она есть.
    pub side: Side,
    /// Вид поверхности.
    pub surface: Surface,
    /// Материал вокруг оси (отверстие), а не внутри (вал).
    pub inward: bool,
    /// Точка плоскости, точка оси или центр.
    pub origin: [f64; 3],
    /// Нормаль плоскости наружу из тела или направление оси.
    pub dir: [f64; 3],
    /// Радиус цилиндра или сферы; у тора — большой.
    pub radius: f64,
    /// У тора — малый радиус; у конуса — половина угла.
    pub radius2: f64,
    /// Охват вокруг оси.
    pub angle: f64,
    /// Границы вдоль оси от `origin`.
    pub span: (f64, f64),
    /// Площадь, мм².
    pub area: f64,
    /// Центр тяжести грани.
    pub center: [f64; 3],
}

/// Одна строка отчёта: вид «изменено» — цвет «добавлено/убрано» занят
/// материалом, а новое отверстие — это убранный материал.
fn line(subject: String, before: Option<String>, after: Option<String>) -> Change {
    Change {
        kind: ChangeKind::Modified,
        subject,
        before,
        after,
    }
}

/// Одинаковые строки подряд — одной строкой с числом: «…, 4 шт.».
fn merged(lines: Vec<Change>) -> Vec<Change> {
    let mut out: Vec<(Change, usize)> = Vec::new();
    for l in lines {
        match out.iter_mut().find(|(c, _)| *c == l) {
            Some((_, n)) => *n += 1,
            None => out.push((l, 1)),
        }
    }
    out.into_iter()
        .map(|(mut c, n)| {
            if n > 1 {
                c.subject = format!("{}, {n} шт.", c.subject);
            }
            c
        })
        .collect()
}

/// Описать изменения словами. `tolerance_mm` — допуск проекта: что ближе,
/// считается тем же местом и тем же размером.
pub fn describe(faces: &[Face], tolerance_mm: f64) -> Vec<Change> {
    let tol = tolerance_mm.max(1e-6);
    let mut lines = Vec::new();
    let mut rest = (0usize, 0usize);
    let mut count = |f: &Face| match f.side {
        Side::Before => rest.0 += 1,
        Side::After => rest.1 += 1,
    };

    let cylinders: Vec<Face> = faces
        .iter()
        .filter(|f| f.surface == Surface::Cylinder)
        .copied()
        .collect();
    for f in round::describe(&cylinders, tol, &mut lines) {
        count(&f);
    }
    let planes: Vec<Face> = faces
        .iter()
        .filter(|f| f.surface == Surface::Plane)
        .copied()
        .collect();
    for f in flat::describe(&planes, tol, &mut lines) {
        count(&f);
    }
    for f in faces {
        if !matches!(f.surface, Surface::Cylinder | Surface::Plane) {
            count(f);
        }
    }

    let mut out = merged(lines);
    if rest != (0, 0) {
        out.push(line(
            "Другие изменённые грани".into(),
            Some(rest.0.to_string()),
            Some(rest.1.to_string()),
        ));
    }
    out
}

#[cfg(test)]
mod tests;
