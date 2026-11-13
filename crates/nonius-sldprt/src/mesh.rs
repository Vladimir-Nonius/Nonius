//! Сетка показа из куска `Contents/DisplayLists` и числа формы по ней.
//!
//! SolidWorks хранит в файле треугольники, которыми рисует деталь на
//! экране. На каждую грань тела — своя запись (числа little-endian):
//!
//! ```text
//! 04 00 00 00  08 00 00 00  02 00 00 00  N       длины N полос
//! u32 × N                                        сколько вершин в полосе
//! 0c 00 00 00  64 00 00 00  02 00 00 00  M       M — сумма длин полос
//! f32 × 3 × M                                    вершины, метры
//! 0c 00 00 00  64 00 00 00  02 00 00 00  M
//! f32 × 3 × M                                    нормали в вершинах
//! ```
//!
//! Полоса — лента треугольников: каждые три соседние вершины — треугольник.
//! Остальное содержимое куска (свет, шрифты, свойства тел) не разбираем:
//! записи граней находим по этому рисунку и проверяем числами `N` и `M`.
//!
//! Числа приблизительные: круглое на экране нарисовано многогранником.
//! Объём поправляем по нормалям (см. [`bulge`]) — тогда он сходится с
//! точным обычно до 1 %; площадь и габариты — как у многогранника.

/// Заголовок списка длин полос.
const STRIPS: [u8; 12] = [4, 0, 0, 0, 8, 0, 0, 0, 2, 0, 0, 0];
/// Заголовок списка троек чисел (вершины, нормали).
const TRIPLES: [u8; 12] = [12, 0, 0, 0, 100, 0, 0, 0, 2, 0, 0, 0];
const HEAD: usize = 16;
/// Нормаль в вершине, которая лежит почти в плоскости треугольника, —
/// это край грани, а не кривизна: поправку по такой не считаем.
const MIN_FACING: f64 = 0.5;
/// Замкнутая сетка: площади, сложенные как векторы, дают ноль. Больше
/// этой доли — в сетке дыра или лишние поверхности, объёму верить нельзя.
const MAX_LEAK: f64 = 0.01;

type V = [f64; 3];

/// Числа формы по сетке: мм, мм², мм³.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    /// Объём, мм³.
    pub volume: f64,
    /// Площадь, мм².
    pub area: f64,
    /// Минимальный угол габаритов, мм.
    pub min: [f64; 3],
    /// Максимальный угол габаритов, мм.
    pub max: [f64; 3],
    /// Число граней.
    pub faces: u32,
}

/// Одна грань: полосы треугольников с нормалями в вершинах.
struct Face {
    strips: Vec<usize>,
    points: Vec<V>,
    normals: Vec<V>,
}

fn u32_at(data: &[u8], at: usize) -> Option<usize> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?) as usize)
}

fn triples(data: &[u8], at: usize, count: usize) -> Option<Vec<V>> {
    let bytes = data.get(at..at.checked_add(count.checked_mul(12)?)?)?;
    let number = |b: &[u8]| f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    Some(
        bytes
            .chunks_exact(12)
            .map(|c| [number(&c[0..4]), number(&c[4..8]), number(&c[8..12])])
            .collect(),
    )
}

fn is_head(data: &[u8], at: usize, head: &[u8; 12]) -> bool {
    data.get(at..at.saturating_add(12)) == Some(head.as_slice())
}

/// Запись грани по адресу `at`; вместе с ней — адрес сразу за записью.
fn face_at(data: &[u8], at: usize) -> Option<(Face, usize)> {
    let count = u32_at(data, at + 12)?;
    let lengths = at + HEAD;
    let points_head = lengths.checked_add(count.checked_mul(4)?)?;
    if count == 0 || !is_head(data, points_head, &TRIPLES) {
        return None;
    }
    let strips: Vec<usize> = (0..count)
        .map(|i| u32_at(data, lengths + 4 * i))
        .collect::<Option<_>>()?;
    let total = u32_at(data, points_head + 12)?;
    if strips
        .iter()
        .try_fold(0usize, |sum, s| sum.checked_add(*s))?
        != total
    {
        return None;
    }
    let points = triples(data, points_head + HEAD, total)?;
    let normals_head = points_head + HEAD + 12 * total;
    if !is_head(data, normals_head, &TRIPLES) || u32_at(data, normals_head + 12)? != total {
        return None;
    }
    let normals = triples(data, normals_head + HEAD, total)?;
    let face = Face {
        strips,
        points,
        normals,
    };
    Some((face, normals_head + HEAD + 12 * total))
}

fn faces(data: &[u8]) -> Vec<Face> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = data
        .get(from..)
        .and_then(|rest| rest.windows(STRIPS.len()).position(|w| w == STRIPS))
        .map(|p| p + from)
    {
        match face_at(data, at) {
            Some((face, end)) => {
                found.push(face);
                from = end;
            }
            None => from = at + 4,
        }
    }
    found
}

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Сколько объёма прячется между плоским треугольником и кривой гранью
/// над ним, на единицу площади треугольника. `unit` — единичная нормаль
/// треугольника; знак ответа — вдоль неё.
///
/// Нормали в вершинах говорят, как грань наклонена к треугольнику на его
/// углах. По двум наклонам на концах ребра высота дуги над серединой
/// ребра — `длина · (наклон в начале − наклон в конце) / 8`; средняя
/// высота грани над треугольником — треть суммы по трём рёбрам.
fn bulge(points: [V; 3], normals: [V; 3], unit: V) -> f64 {
    let mut height = 0.0;
    for i in 0..3 {
        let j = (i + 1) % 3;
        let edge = sub(points[j], points[i]);
        let (facing_i, facing_j) = (dot(normals[i], unit), dot(normals[j], unit));
        if facing_i.abs() < MIN_FACING || facing_j.abs() < MIN_FACING {
            return 0.0;
        }
        height += (dot(normals[j], edge) / facing_j - dot(normals[i], edge) / facing_i) / 8.0;
    }
    height / 3.0
}

/// Сумма по сетке: объём, площадь, «утечка» (площади как векторы).
#[derive(Default)]
struct Sum {
    volume: f64,
    area: f64,
    leak: V,
}

impl Sum {
    fn add(&mut self, points: [V; 3], normals: [V; 3]) {
        let twice = cross(sub(points[1], points[0]), sub(points[2], points[0]));
        let size = dot(twice, twice).sqrt();
        if size == 0.0 || !size.is_finite() {
            return;
        }
        let unit = twice.map(|x| x / size);
        let outward: V = std::array::from_fn(|k| normals.iter().map(|n| n[k]).sum());
        // В ленте треугольники смотрят то в одну сторону, то в другую;
        // где у детали «наружу», знают нормали.
        let sign = if dot(unit, outward) >= 0.0 { 1.0 } else { -1.0 };
        self.area += size / 2.0;
        self.volume += sign * dot(points[0], twice) / 6.0;
        self.volume += sign * (size / 2.0) * bulge(points, normals, unit);
        for (leak, part) in self.leak.iter_mut().zip(twice) {
            *leak += sign * part / 2.0;
        }
    }
}

/// Числа формы по куску `Contents/DisplayLists`. `None` — сетки нет, она
/// не замкнута или вывернута: объём по такой не посчитать.
pub fn measure(display_lists: &[u8]) -> Option<Measured> {
    let faces = faces(display_lists);
    let mut sum = Sum::default();
    let (mut min, mut max) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for face in &faces {
        let mut start = 0;
        for length in &face.strips {
            for a in start..(start + length).saturating_sub(2) {
                sum.add(
                    [face.points[a], face.points[a + 1], face.points[a + 2]],
                    [face.normals[a], face.normals[a + 1], face.normals[a + 2]],
                );
            }
            start += length;
        }
        for point in &face.points {
            for k in 0..3 {
                min[k] = min[k].min(point[k]);
                max[k] = max[k].max(point[k]);
            }
        }
    }
    let closed = dot(sum.leak, sum.leak).sqrt() <= MAX_LEAK * sum.area;
    let finite = sum.volume.is_finite()
        && sum.area.is_finite()
        && min.iter().chain(&max).all(|x| x.is_finite());
    if faces.is_empty() || !finite || !closed || sum.volume <= 0.0 {
        return None;
    }
    // В файле метры; у Nonius — миллиметры.
    Some(Measured {
        volume: sum.volume * 1e9,
        area: sum.area * 1e6,
        min: min.map(|x| x * 1e3),
        max: max.map(|x| x * 1e3),
        faces: u32::try_from(faces.len()).ok()?,
    })
}
