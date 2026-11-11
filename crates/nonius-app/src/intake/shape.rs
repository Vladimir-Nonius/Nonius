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
//! именам, на настоящие детали, [`hints`] подсказывает про похожие детали с
//! непохожими именами (ничего не объединяя — решает человек).

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
/// Сколько имён перечислять в подсказке.
const NAMED: usize = 3;

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
    /// Что сказала форма; `None` — не проверялось.
    pub verdict: Option<ShapeVerdict>,
    /// Строки для «почему».
    pub notes: Vec<String>,
}

/// Проверить формой группу `members`, собранную по именам (от старой версии
/// к новой). `shape` и `label` отвечают по номеру модели. Если хоть у одной
/// модели формы нет — группа остаётся, как была, с пометкой «не проверено».
pub(super) fn split(
    members: &[usize],
    shape: &dyn Fn(usize) -> Option<Shape>,
    label: &dyn Fn(usize) -> String,
) -> Vec<Cluster> {
    let shapes: Vec<Shape> = members.iter().filter_map(|&m| shape(m)).collect();
    if shapes.len() != members.len() {
        return vec![Cluster {
            members: members.to_vec(),
            verdict: None,
            notes: Vec::new(),
        }];
    }
    // Похожие — в одну кучку; похожесть тянется по цепочке версий.
    let mut heap: Vec<usize> = (0..members.len()).collect();
    for a in 0..members.len() {
        for b in a + 1..members.len() {
            if !matches!(compare(&shapes[a], &shapes[b]), Likeness::Different(_)) {
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
            let mut notes = Vec::new();
            let verdict = if let [only] = at[..] {
                // Отделилась: сказать, от кого и чем отличается.
                let other = if only == 0 { 1 } else { 0 };
                if let Likeness::Different(how) = compare(&shapes[other], &shapes[only]) {
                    notes.push(format!(
                        "имя похоже на «{}», но форма другая: {how}",
                        label(members[other])
                    ));
                }
                ShapeVerdict::Different
            } else {
                let mut copies = 0;
                for pair in at.windows(2) {
                    let (old, new) = (label(members[pair[0]]), label(members[pair[1]]));
                    match compare(&shapes[pair[0]], &shapes[pair[1]]) {
                        Likeness::Same => {
                            copies += 1;
                            notes.push(format!(
                                "форма у «{new}» такая же, как у «{old}»: возможно, это копия"
                            ));
                        }
                        Likeness::Similar(how) => {
                            notes.push(format!("подтверждено формой, «{old}» → «{new}»: {how}"));
                        }
                        Likeness::Different(how) => {
                            notes.push(format!("«{old}» → «{new}» изменилась сильно: {how}"));
                        }
                    }
                }
                if copies > 0 {
                    ShapeVerdict::Copy
                } else {
                    ShapeVerdict::Confirmed
                }
            };
            Cluster {
                members: at.iter().map(|&i| members[i]).collect(),
                verdict: Some(verdict),
                notes,
            }
        })
        .collect()
}

/// Подсказки про детали, которые по именам не связаны, а формой похожи.
/// `parts` — по одной записи на будущую деталь: имя и форма её последней
/// версии; `apart(a, b)` — эти две точно разные (стоят в одной сборке).
/// Ответ — строки для «почему» каждой детали. Ничего не объединяется.
pub(super) fn hints(
    parts: &[(String, Option<Shape>)],
    apart: &dyn Fn(usize, usize) -> bool,
) -> Vec<Vec<String>> {
    let mut same: Vec<Vec<&str>> = vec![Vec::new(); parts.len()];
    let mut alike: Vec<Vec<&str>> = vec![Vec::new(); parts.len()];
    for a in 0..parts.len() {
        for b in a + 1..parts.len() {
            let (Some(x), Some(y)) = (&parts[a].1, &parts[b].1) else {
                continue;
            };
            if apart(a, b) {
                continue;
            }
            let lists = match compare(x, y) {
                Likeness::Same => &mut same,
                Likeness::Similar(_) => &mut alike,
                Likeness::Different(_) => continue,
            };
            lists[a].push(&parts[b].0);
            lists[b].push(&parts[a].0);
        }
    }
    let listed = |names: &[&str]| {
        let mut text = names
            .iter()
            .take(NAMED)
            .map(|n| format!("«{n}»"))
            .collect::<Vec<_>>()
            .join(", ");
        if names.len() > NAMED {
            text.push_str(&format!(" и ещё {}", names.len() - NAMED));
        }
        text
    };
    (0..parts.len())
        .map(|i| {
            let mut notes = Vec::new();
            if !same[i].is_empty() {
                notes.push(format!(
                    "форма такая же, как у {}: возможно, это копии одной детали",
                    listed(&same[i])
                ));
            }
            if !alike[i].is_empty() {
                notes.push(format!(
                    "по форме похожа на {}: возможно, это версии одной детали",
                    listed(&alike[i])
                ));
            }
            notes
        })
        .collect()
}

#[cfg(test)]
#[path = "shape_tests.rs"]
mod tests;
