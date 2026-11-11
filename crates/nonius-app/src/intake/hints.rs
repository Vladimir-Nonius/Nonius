//! Подсказки про детали, которые по именам не связаны, а формой похожи
//! или одинаковы. Ничего не объединяется: это повод для вопроса человеку
//! (`questions.rs`) или просто строка в «почему».

use super::shape::{compare, Likeness, Shape};

/// Сколько имён перечислять в подсказке.
const NAMED: usize = 3;

/// Что нашлось.
pub(super) struct Hints {
    /// Строки для «почему» — по каждой детали.
    pub notes: Vec<Vec<String>>,
    /// Пары похожих (не одинаковых) деталей и чем они похожи.
    pub alike: Vec<(usize, usize, String)>,
}

/// `parts` — по одной записи на будущую деталь: имя и форма её последней
/// версии; `apart(a, b)` — эти две точно разные (стоят в одной сборке или
/// так сказал человек).
pub(super) fn hints(
    parts: &[(String, Option<Shape>)],
    apart: &dyn Fn(usize, usize) -> bool,
) -> Hints {
    let mut same: Vec<Vec<&str>> = vec![Vec::new(); parts.len()];
    let mut alike: Vec<Vec<&str>> = vec![Vec::new(); parts.len()];
    let mut pairs = Vec::new();
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
                Likeness::Similar(how) => {
                    pairs.push((a, b, how));
                    &mut alike
                }
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
    let notes = (0..parts.len())
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
        .collect();
    Hints {
        notes,
        alike: pairs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(volume: f64, dims: [f64; 3]) -> Shape {
        Shape {
            volume,
            area: None,
            dims,
            faces: None,
        }
    }

    fn manifold() -> (Shape, Shape) {
        let v1 = shape(439_112.9, [61.0, 148.0, 420.0]);
        (v1, shape(435_724.8, [61.0, 148.0, 420.0]))
    }

    #[test]
    fn hints_name_look_alikes() {
        let (v1, v2) = manifold();
        let plug = shape(300.0, [5.0, 10.0, 10.0]);
        let mut parts = vec![
            ("крышка".to_string(), Some(v1)),
            ("крышка доработанная".to_string(), Some(v2)),
            ("кронштейн".to_string(), None),
        ];
        for n in 1..=5 {
            parts.push((format!("заглушка{n}"), Some(plug)));
        }
        let found = hints(&parts, &|_, _| false);
        let notes = found.notes;
        assert_eq!(found.alike.len(), 1);
        assert_eq!((found.alike[0].0, found.alike[0].1), (0, 1));
        assert_eq!(
            notes[0],
            ["по форме похожа на «крышка доработанная»: возможно, это версии одной детали"]
        );
        assert_eq!(notes[1].len(), 1);
        assert!(notes[2].is_empty(), "без формы подсказок нет");
        assert_eq!(
            notes[3],
            [
                "форма такая же, как у «заглушка2», «заглушка3», «заглушка4» и ещё 1: \
              возможно, это копии одной детали"
            ]
        );
        // Стоят вместе в сборке — точно разные детали, подсказка не нужна.
        let notes = hints(&parts, &|_, _| true).notes;
        assert!(notes.iter().all(Vec::is_empty));
    }
}
