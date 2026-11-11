//! Деление группы моделей с одним именем на настоящие детали.
//!
//! В группу попадает всё, что названо одинаково: `вал` и `вал_v2` из одной
//! папки, `вал` из папки «рабочая версия» и `вал` из папки «готовая». Кто
//! из них версии одной детали, решают свидетельства ([`Evidence`]), от
//! сильного к слабому:
//! 1. стоят вместе в одной сборке — точно разные детали;
//! 2. ответ человека на вопрос;
//! 3. форма (`shape.rs`): похожая — версии, разная — разные детали;
//! 4. формы нет — имя: в одной папке ему верим, в разных — нет.
//!
//! Одно имя в разных папках и одинаковая форма — это копия проекта, а не
//! новая версия: такая модель уходит в `dupes` и версией не становится.

use super::shape::{compare, Likeness, Shape, ShapeVerdict};

/// Свидетельства о моделях группы; все отвечают по номеру модели.
pub(super) struct Evidence<'a> {
    /// Отпечаток формы, если его сняли.
    pub shape: &'a dyn Fn(usize) -> Option<Shape>,
    /// Как назвать модель человеку.
    pub label: &'a dyn Fn(usize) -> String,
    /// Что человек ответил про пару.
    pub decided: &'a dyn Fn(usize, usize) -> Option<bool>,
    /// Точно разные детали: стоят вместе в одной сборке.
    pub apart: &'a dyn Fn(usize, usize) -> bool,
    /// Формы нет — хватит ли одного имени, чтобы считать пару версиями.
    pub trust: &'a dyn Fn(usize, usize) -> bool,
    /// Одно имя в разных папках: с той же формой это копия, а не версия.
    pub twin: &'a dyn Fn(usize, usize) -> bool,
}

/// Настоящая деталь внутри группы.
pub(super) struct Cluster {
    /// Версии от старой к новой.
    pub members: Vec<usize>,
    /// Копии версий из других папок: в проект не идут.
    pub dupes: Vec<usize>,
    /// Что сказала форма; `None` — не проверялось или решил человек.
    pub verdict: Option<ShapeVerdict>,
    /// Строки для «почему».
    pub notes: Vec<String>,
    /// Модель, от которой эту отделила форма: о паре стоит спросить.
    pub rival: Option<usize>,
    /// Все версии подтвердил человек: спрашивать больше не о чем.
    pub settled: bool,
}

/// Разделить группу `members` (от старой версии к новой).
pub(super) fn split(members: &[usize], ev: &Evidence) -> Vec<Cluster> {
    let n = members.len();
    let shapes: Vec<Option<Shape>> = members.iter().map(|&m| (ev.shape)(m)).collect();
    let likeness = |a: usize, b: usize| match (&shapes[a], &shapes[b]) {
        (Some(x), Some(y)) => Some(compare(x, y)),
        _ => None,
    };
    let said = |a: usize, b: usize| (ev.decided)(members[a], members[b]);
    let blocked =
        |a: usize, b: usize| (ev.apart)(members[a], members[b]) || said(a, b) == Some(false);
    let linked = |a: usize, b: usize| {
        said(a, b).unwrap_or_else(|| match likeness(a, b) {
            Some(like) => !matches!(like, Likeness::Different(_)),
            None => (ev.trust)(members[a], members[b]),
        })
    };
    // Похожие — в одну кучку; похожесть тянется по цепочке версий, но
    // точно разные детали в одной кучке не окажутся.
    let mut heap: Vec<usize> = (0..n).collect();
    for a in 0..n {
        for b in a + 1..n {
            if heap[a] == heap[b] || blocked(a, b) || !linked(a, b) {
                continue;
            }
            let (from, to) = (heap[b], heap[a]);
            let clash = (0..n)
                .filter(|&x| heap[x] == from)
                .any(|x| (0..n).filter(|&y| heap[y] == to).any(|y| blocked(x, y)));
            if !clash {
                heap.iter_mut()
                    .filter(|h| **h == from)
                    .for_each(|h| *h = to);
            }
        }
    }
    let mut heads: Vec<usize> = heap.clone();
    heads.sort_unstable();
    heads.dedup();
    heads
        .into_iter()
        .map(|head| {
            let at: Vec<usize> = (0..n).filter(|&i| heap[i] == head).collect();
            cluster(members, &at, ev, &likeness, &said)
        })
        .collect()
}

fn cluster(
    members: &[usize],
    at: &[usize],
    ev: &Evidence,
    likeness: &dyn Fn(usize, usize) -> Option<Likeness>,
    said: &dyn Fn(usize, usize) -> Option<bool>,
) -> Cluster {
    let label = |i: usize| (ev.label)(members[i]);
    let mut part = Cluster {
        members: Vec::new(),
        dupes: Vec::new(),
        verdict: None,
        notes: Vec::new(),
        rival: None,
        settled: false,
    };
    if let [only] = at[..] {
        part.members.push(members[only]);
        let others = || (0..members.len()).filter(|&o| o != only);
        if let Some(other) = others().find(|&o| said(only, o) == Some(false)) {
            part.notes
                .push(format!("вы ответили: это не версия «{}»", label(other)));
            return part;
        }
        // Отделила форма: сказать, от кого и чем отличается.
        let differs = others().find_map(|o| match likeness(o, only) {
            Some(Likeness::Different(how)) => Some((o, how)),
            _ => None,
        });
        if let Some((other, how)) = differs {
            part.notes.push(format!(
                "имя похоже на «{}», но форма другая: {how}",
                label(other)
            ));
            part.verdict = Some(ShapeVerdict::Different);
            // Спрашивать стоит про соседей по папке; одинаковых имён в
            // разных папках слишком много.
            if (ev.trust)(members[other], members[only]) {
                part.rival = Some(members[other]);
            }
        }
        return part;
    }
    let (mut copies, mut unsure) = (0, false);
    let mut kept: Vec<usize> = Vec::new();
    part.settled = true;
    for &new in at {
        let Some(&old) = kept.last() else {
            kept.push(new);
            continue;
        };
        let like = likeness(old, new);
        if like == Some(Likeness::Same) && (ev.twin)(members[old], members[new]) {
            // Остаётся более новый файл: если отличие и есть, оно в нём.
            part.dupes.push(members[old]);
            kept.pop();
            kept.push(new);
            continue;
        }
        part.settled &= said(old, new) == Some(true);
        match like {
            Some(Likeness::Same) => {
                copies += 1;
                part.notes.push(format!(
                    "форма у «{}» такая же, как у «{}»: возможно, это копия",
                    label(new),
                    label(old)
                ));
            }
            Some(Likeness::Similar(how)) => part.notes.push(format!(
                "подтверждено формой, «{}» → «{}»: {how}",
                label(old),
                label(new)
            )),
            Some(Likeness::Different(how)) => {
                unsure = true;
                part.notes.push(format!(
                    "«{}» → «{}» изменилась сильно: {how}",
                    label(old),
                    label(new)
                ));
            }
            None => unsure = true,
        }
        kept.push(new);
    }
    part.members = kept.iter().map(|&i| members[i]).collect();
    if part.members.len() < 2 {
        part.settled = false;
        return part;
    }
    if part.settled {
        part.notes
            .push("вы ответили: это версии одной детали".into());
    }
    part.verdict = match (unsure, copies) {
        (true, _) => None,
        (false, 0) => Some(ShapeVerdict::Confirmed),
        (false, _) => Some(ShapeVerdict::Copy),
    };
    part
}
