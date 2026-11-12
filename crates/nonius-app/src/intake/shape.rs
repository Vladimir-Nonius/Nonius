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
/// Копия: все числа совпали с точностью записи в файл.
const COPY: f64 = 1e-4;
/// Копия, когда одну форму сняли точно (по STEP), а другую — бегло, по
/// сетке показа из файла: способы расходятся до 2 %, мельче не разглядеть.
const COPY_MIXED: f64 = 0.02;

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
    /// Числа сняты бегло — по сетке показа из самого файла CAD, а не по
    /// STEP. Они чуть грубее, а граней в сетке меньше, чем в STEP, поэтому
    /// с точной формой такая сравнивается мягче (см. [`compare`]).
    pub rough: bool,
}

impl Shape {
    /// Отпечаток из паспорта. Нет объёма или габаритов — сравнивать нечем.
    pub fn of(g: &GeometrySummary) -> Option<Self> {
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
            rough: false,
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
    let sizes: Vec<f64> = (0..3).map(|i| gap(a.dims[i], b.dims[i])).collect();
    let worst = sizes.iter().copied().fold(0.0, f64::max);
    let volume = gap(a.volume, b.volume);
    let area = match (a.area, b.area) {
        (Some(x), Some(y)) => gap(x, y),
        _ => 0.0,
    };
    // Точная форма и беглая — числа из разных источников: грани не
    // сравниваем вовсе, остальное — с запасом на расхождение способов.
    let mixed = a.rough != b.rough;
    let faces = match (a.faces, b.faces) {
        (Some(x), Some(y)) if !mixed => x == y,
        _ => true,
    };
    let copy = if mixed { COPY_MIXED } else { COPY };
    if worst < copy && volume < copy && area < copy && faces {
        return Likeness::Same;
    }
    let percent = format!(
        "объём {}{} %",
        if b.volume >= a.volume { "+" } else { "−" },
        number(volume * 100.0, 1)
    );
    let kept = sizes.iter().filter(|s| **s <= SAME_SIZE).count();
    let resized = kept >= 2 && worst <= MAX_CHANGE;
    if worst > TOUCHED && !resized {
        return Likeness::Different(format!("габариты {} и {}", a.size(), b.size()));
    }
    if volume > MAX_CHANGE {
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
