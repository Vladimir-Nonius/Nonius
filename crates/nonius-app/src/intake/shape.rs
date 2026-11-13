//! Проверка догадок формой: правда ли `вал` и `вал_v2` — одна деталь.
//!
//! Имя файла — слабое свидетельство. Сильное — сама деталь: у версий одной
//! детали почти те же габариты и близкий объём. Здесь нет ни диска, ни
//! SolidWorks: «отпечаток» формы ([`Shape`]) снимает `measure.rs` — из
//! самого файла CAD или из паспорта модели, — а тут его только сравнивают.
//!
//! Три ответа ([`Likeness`]):
//! - одинаковая форма — это копия файла, а не новая версия;
//! - похожая — версии одной детали: объём ушёл не дальше четверти, а
//!   габариты либо все изменились слегка (до 10 %), либо два из трёх те же
//!   и третий ушёл не дальше четверти (вал удлинили);
//! - разная — разные детали, хоть имена и похожи.
//!
//! Из ответов получаются решения: `split.rs` делит группу моделей с одним
//! именем на настоящие детали (слово человека при этом сильнее формы), а
//! `hints.rs` подсказывает про похожие детали с непохожими именами.

use nonius_core::meta::GeometrySummary;
use serde::Serialize;

/// Габарит «тот же»: расхождение не больше 2 %.
const SAME_SIZE: f64 = 0.02;
/// Габариты «поправили»: все три ушли не дальше 10 %. Порог — по настоящей
/// доработке впускного коллектора: 68×90×398 → 70×84×398 мм.
const TOUCHED: f64 = 0.10;
/// Насколько может уйти третий габарит и объём у версий одной детали.
const MAX_CHANGE: f64 = 0.25;
/// Форма «другая», но два габарита из трёх те же: так выглядит и деталь,
/// которую укоротили или утолщили. По этим словам вопрос человеку задаётся
/// без мнения «похоже, нет» (см. `questions.rs`).
pub(super) const TWO_KEPT: &str = "два габарита из трёх совпали";
/// Копия: все числа совпали с точностью записи в файл.
const COPY: f64 = 1e-4;
/// Насколько беглая форма может разойтись с точной: габариты по сетке —
/// до 1 %, объём — до 2 % (сверка 101 детали с SolidWorks).
const GLANCE_ERROR: f64 = 0.02;

/// Откуда сняты числа формы.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Source {
    /// По STEP: от SolidWorks или готовый рядом с файлом.
    Exact,
    /// Беглый взгляд: по сетке показа из самого файла CAD (`ShapeGlance`).
    /// Числа чуть грубее, а граней в сетке меньше, чем в STEP.
    Glance,
}

/// Чего ждать от сравнения двух форм — зависит только от того, откуда
/// сняты числа. Все поправки на беглую форму — здесь, и больше нигде.
struct Limits {
    /// Ниже этого расхождения — копия.
    copy: f64,
    /// Число граней сравнимо: обе формы из одного источника.
    faces: bool,
    /// На сколько ответ может ошибаться у порогов: у точных форм — ни на
    /// сколько, если хоть одна беглая — на [`GLANCE_ERROR`].
    error: f64,
}

impl Limits {
    fn between(a: Source, b: Source) -> Self {
        let error = if a == Source::Exact && b == Source::Exact {
            0.0
        } else {
            GLANCE_ERROR
        };
        // Одна форма точная, другая беглая — способы расходятся, и копию
        // мельче их расхождения не разглядеть. Обе беглые — источник один:
        // копия даёт те же числа.
        let mixed = a != b;
        Self {
            copy: if mixed { GLANCE_ERROR } else { COPY },
            faces: !mixed,
            error,
        }
    }
}

/// Что форма говорит о версиях детали.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeVerdict {
    /// Версии похожи формой: догадка по именам подтверждена.
    Confirmed,
    /// Среди версий есть одинаковые: возможно, это копии файла.
    Copy,
    /// Имя похоже на другую деталь, но форма другая: деталь отделена.
    Different,
}

/// Отпечаток формы: по нему видно, одна ли это деталь.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Shape {
    /// Объём, мм³.
    pub volume: f64,
    /// Площадь, мм².
    pub area: Option<f64>,
    /// Габариты по возрастанию, мм: как деталь повёрнута — не важно.
    pub dims: [f64; 3],
    /// Число граней.
    pub faces: Option<u32>,
    /// Откуда числа: от этого зависит, насколько им верить (см. [`Limits`]).
    pub source: Source,
}

impl Shape {
    /// Отпечаток из паспорта. Нет объёма или габаритов — сравнивать нечем.
    pub fn of(g: &GeometrySummary, source: Source) -> Option<Self> {
        let volume = g.volume_mm3.filter(|v| *v > 0.0)?;
        let b = g.bbox?;
        let mut dims = [
            b.max[0] - b.min[0],
            b.max[1] - b.min[1],
            b.max[2] - b.min[2],
        ];
        dims.sort_by(f64::total_cmp);
        Some(Self {
            volume,
            area: g.area_mm2,
            dims,
            faces: g.faces,
            source,
        })
    }

    fn size(&self) -> String {
        let mm = |v: f64| number(v, if v < 10.0 { 1 } else { 0 });
        format!(
            "{}×{}×{} мм",
            mm(self.dims[0]),
            mm(self.dims[1]),
            mm(self.dims[2])
        )
    }
}

/// Насколько две формы похожи.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Likeness {
    /// Одна и та же форма.
    Same,
    /// Версии одной детали; внутри — что изменилось, словами.
    Similar(String),
    /// Разные детали; внутри — чем отличаются, словами.
    Different(String),
}

/// Число по-русски: с запятой.
fn number(v: f64, digits: usize) -> String {
    format!("{v:.digits$}").replace('.', ",")
}

/// Доля расхождения двух чисел от большего из них.
fn gap(a: f64, b: f64) -> f64 {
    (a - b).abs() / a.abs().max(b.abs()).max(1e-9)
}

/// Сравнить форму «было» и «стало».
pub(super) fn compare(a: &Shape, b: &Shape) -> Likeness {
    judge(a, b, 0.0)
}

/// Ответ сравнения на грани: беглая форма ошибается на проценты, и при
/// чуть других числах ответ был бы другим (похожи ↔ разные, копия ↔
/// версия). Такие пары стоит досчитать точно. Точные формы спорными не
/// бывают.
pub(super) fn doubtful(a: &Shape, b: &Shape) -> bool {
    let error = Limits::between(a.source, b.source).error;
    let kind = |l: &Likeness| std::mem::discriminant(l);
    error > 0.0 && kind(&judge(a, b, -error)) != kind(&judge(a, b, error))
}

/// Сравнение с порогами, сдвинутыми на `shift` (0 — как есть).
fn judge(a: &Shape, b: &Shape, shift: f64) -> Likeness {
    let limits = Limits::between(a.source, b.source);
    let sizes: Vec<f64> = (0..3).map(|i| gap(a.dims[i], b.dims[i])).collect();
    let worst = sizes.iter().copied().fold(0.0, f64::max);
    let volume = gap(a.volume, b.volume);
    let area = match (a.area, b.area) {
        (Some(x), Some(y)) => gap(x, y),
        _ => 0.0,
    };
    let faces = match (a.faces, b.faces) {
        (Some(x), Some(y)) if limits.faces => x == y,
        _ => true,
    };
    // Точная копия — точная при любом сдвиге; сдвигается только запас на
    // расхождение способов.
    let copy = if limits.copy > COPY {
        (limits.copy + shift).max(COPY)
    } else {
        COPY
    };
    let (same_size, touched, max_change) = (SAME_SIZE + shift, TOUCHED + shift, MAX_CHANGE + shift);
    if worst < copy && volume < copy && area < copy && faces {
        return Likeness::Same;
    }
    let percent = format!(
        "объём {}{} %",
        if b.volume >= a.volume { "+" } else { "−" },
        number(volume * 100.0, 1)
    );
    let kept = sizes.iter().filter(|s| **s <= same_size).count();
    let resized = kept >= 2 && worst <= max_change;
    if worst > touched && !resized {
        let sizes = format!("габариты {} и {}", a.size(), b.size());
        // Габариты отсортированы, поэтому «те же два» ищем без учёта места.
        let mut free = b.dims.to_vec();
        let same = a.dims.iter().filter(|&&d| {
            let hit = free.iter().position(|&o| gap(d, o) <= SAME_SIZE);
            hit.map(|at| free.swap_remove(at)).is_some()
        });
        return Likeness::Different(if same.count() >= 2 {
            format!("{sizes}; {TWO_KEPT}")
        } else {
            sizes
        });
    }
    if volume > max_change {
        return Likeness::Different(percent);
    }
    Likeness::Similar(if kept == 3 {
        format!("{percent}, габариты те же")
    } else {
        format!("{percent}, габариты {} → {}", a.size(), b.size())
    })
}

#[cfg(test)]
#[path = "shape_tests.rs"]
mod tests;
