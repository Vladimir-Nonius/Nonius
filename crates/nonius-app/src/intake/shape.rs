//! Проверка догадок формой: правда ли `вал` и `вал_v2` — одна деталь.
//!
//! Имя файла — слабое свидетельство. Сильное — сама деталь: у версий одной
//! детали почти те же габариты и близкий объём. Здесь нет ни диска, ни
//! SolidWorks: «отпечаток» формы ([`Shape`]) снимает `measure.rs` из
//! паспорта модели, а тут его только сравнивают.
//!
//! Три ответа ([`Likeness`]):
//! - одинаковая форма — это копия файла, а не новая версия;
//! - похожая — версии одной детали: объём ушёл не дальше четверти, а
//!   габариты либо все изменились слегка (до 10 %), либо два из трёх те же
//!   и третий ушёл не дальше четверти (вал удлинили);
//! - разная — разные детали, хоть имена и похожи.
//!
//! Из ответов получаются решения: [`split`] делит группу, собранную по
//! именам, на настоящие детали (слово человека при этом сильнее формы), а
//! `hints.rs` подсказывает про похожие детали с непохожими именами.

use std::collections::BTreeSet;

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
    let faces = match (a.faces, b.faces) {
        (Some(x), Some(y)) => x == y,
        _ => true,
    };
    if worst < COPY && volume < COPY && area < COPY && faces {
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

/// Настоящая деталь внутри группы, собранной по именам.
pub(super) struct Cluster {
    /// Номера моделей — в том же порядке, что пришли.
    pub members: Vec<usize>,
    /// Что сказала форма; `None` — не проверялось или решил человек.
    pub verdict: Option<ShapeVerdict>,
    /// Строки для «почему».
    pub notes: Vec<String>,
    /// Модель, от которой эту отделила форма: о паре стоит спросить.
    pub rival: Option<usize>,
    /// Все версии подтвердил человек: спрашивать больше не о чем.
    pub settled: bool,
}

/// Проверить группу `members`, собранную по именам (от старой версии к
/// новой). `shape` и `label` отвечают по номеру модели, `decided` — что
/// человек сказал про пару моделей: его слово сильнее формы. Форма судит,
/// только если она есть у всех моделей группы; иначе верим именам.
pub(super) fn split(
    members: &[usize],
    shape: &dyn Fn(usize) -> Option<Shape>,
    label: &dyn Fn(usize) -> String,
    decided: &dyn Fn(usize, usize) -> Option<bool>,
) -> Vec<Cluster> {
    let shapes: Vec<Option<Shape>> = members.iter().map(|&m| shape(m)).collect();
    let checked = shapes.iter().all(Option::is_some);
    let likeness = |a: usize, b: usize| match (&shapes[a], &shapes[b]) {
        (Some(x), Some(y)) if checked => Some(compare(x, y)),
        _ => None,
    };
    let said = |a: usize, b: usize| decided(members[a], members[b]);
    // Похожие — в одну кучку; похожесть тянется по цепочке версий.
    let mut heap: Vec<usize> = (0..members.len()).collect();
    for a in 0..members.len() {
        for b in a + 1..members.len() {
            let together = said(a, b)
                .unwrap_or_else(|| !matches!(likeness(a, b), Some(Likeness::Different(_))));
            if together {
                let (from, to) = (heap[b], heap[a]);
                heap.iter_mut()
                    .filter(|h| **h == from)
                    .for_each(|h| *h = to);
            }
        }
    }
    let heads: BTreeSet<usize> = heap.iter().copied().collect();
    heads
        .into_iter()
        .map(|head| {
            let at: Vec<usize> = (0..members.len()).filter(|&i| heap[i] == head).collect();
            let mut part = Cluster {
                members: at.iter().map(|&i| members[i]).collect(),
                verdict: None,
                notes: Vec::new(),
                rival: None,
                settled: false,
            };
            if let [only] = at[..] {
                let refused = (0..members.len()).find(|&o| said(only, o) == Some(false));
                if let Some(other) = refused {
                    part.notes.push(format!(
                        "вы ответили: это не версия «{}»",
                        label(members[other])
                    ));
                    return part;
                }
                // Отделила форма: сказать, от кого и чем отличается.
                let other = usize::from(only == 0);
                if let Some(Likeness::Different(how)) = likeness(other, only) {
                    part.notes.push(format!(
                        "имя похоже на «{}», но форма другая: {how}",
                        label(members[other])
                    ));
                }
                part.verdict = Some(ShapeVerdict::Different);
                part.rival = Some(members[other]);
                return part;
            }
            let (mut copies, mut odd) = (0, false);
            part.settled = true;
            for pair in at.windows(2) {
                let (old, new) = (label(members[pair[0]]), label(members[pair[1]]));
                part.settled &= said(pair[0], pair[1]) == Some(true);
                match likeness(pair[0], pair[1]) {
                    Some(Likeness::Same) => {
                        copies += 1;
                        part.notes.push(format!(
                            "форма у «{new}» такая же, как у «{old}»: возможно, это копия"
                        ));
                    }
                    Some(Likeness::Similar(how)) => part
                        .notes
                        .push(format!("подтверждено формой, «{old}» → «{new}»: {how}")),
                    Some(Likeness::Different(how)) => {
                        odd = true;
                        part.notes
                            .push(format!("«{old}» → «{new}» изменилась сильно: {how}"));
                    }
                    None => {}
                }
            }
            if part.settled {
                part.notes
                    .push("вы ответили: это версии одной детали".into());
            }
            part.verdict = match (checked && !odd, copies) {
                (false, _) => None,
                (true, 0) => Some(ShapeVerdict::Confirmed),
                (true, _) => Some(ShapeVerdict::Copy),
            };
            part
        })
        .collect()
}

#[cfg(test)]
#[path = "shape_tests.rs"]
mod tests;
