//! Вопросы человеку и его ответы: «диалоговый режим» разбора.
//!
//! Там, где программа не уверена, она не решает молча, а спрашивает:
//! «Это версии одной детали: «вал» → «вал_v2»?» — и прикладывает улики
//! (что сказала форма, когда файлы менялись). Ответ «да» или «нет»
//! возвращается в правила ([`Decisions`]), и план пересчитывается: слово
//! человека сильнее и имён, и формы.
//!
//! О чём спрашивает:
//! - версии угаданы по именам, а форма их не подтвердила (не проверялась
//!   или одинаковая);
//! - имена похожи, но форма разная — программа развела детали и просит
//!   подтвердить;
//! - имена разные, а форма похожа (глубокая проверка) — только когда пара
//!   одна: двадцать одинаковых заглушек остаются подсказкой, а не
//!   двадцатью вопросами.
//!
//! Вопрос узнаётся по `id` — именам файлов через `|`. Он не меняется от
//! пересчёта к пересчёту, поэтому ответ можно дать и позже, и из другой
//! программы (окно, ИИ, `nonius intake --answer`).

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::grouping::{label, Proto};
use super::shape::ShapeVerdict;
use super::units::Unit;

/// Вопрос человеку. Ответ — «да» или «нет».
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Question {
    /// По нему даётся ответ; от пересчёта плана не меняется.
    pub id: String,
    /// Сам вопрос.
    pub text: String,
    /// Улики: что известно программе.
    pub facts: Vec<String>,
    /// Что думает программа, если у неё есть мнение.
    pub suggested: Option<bool>,
}

/// Ответы человека: про каждую пару моделей — одна это деталь или нет.
#[derive(Debug, Clone, Default)]
pub(super) struct Decisions(HashMap<(String, String), bool>);

impl Decisions {
    /// Из ответов на вопросы: `id` вопроса и «да»/«нет». В вопросе про три
    /// версии ответ относится ко всем парам сразу.
    pub fn new(answers: &[(String, bool)]) -> Self {
        let mut pairs = HashMap::new();
        for (id, same) in answers {
            let ids: Vec<&str> = id.split('|').collect();
            for (n, a) in ids.iter().enumerate() {
                for b in &ids[n + 1..] {
                    pairs.insert(Self::pair(a, b), *same);
                }
            }
        }
        Self(pairs)
    }

    fn pair(a: &str, b: &str) -> (String, String) {
        let (a, b) = (a.to_lowercase(), b.to_lowercase());
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    /// Что человек сказал про эти две модели (по их `Unit::id`).
    pub fn get(&self, a: &str, b: &str) -> Option<bool> {
        self.0.get(&Self::pair(a, b)).copied()
    }
}

/// День по-русски: `05.03.2024`. Без часовых поясов — нужен только порядок.
fn date(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let z = i64::try_from(secs / 86_400).unwrap_or(0) + 719_468;
    let (era, day) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let year = (day - day / 1460 + day / 36_524 - day / 146_096) / 365;
    let in_year = day - (365 * year + year / 4 - year / 100);
    let part = (5 * in_year + 2) / 153;
    let d = in_year - (153 * part + 2) / 5 + 1;
    let m = if part < 10 { part + 3 } else { part - 9 };
    let y = year + era * 400 + i64::from(m <= 2);
    format!("{d:02}.{m:02}.{y}")
}

/// Составить вопросы. `alike` — пары будущих деталей (номера в `protos`),
/// похожих формой при разных именах, и чем они похожи; `changed` — что
/// изменилось между двумя моделями словами (см. `words.rs`).
pub(super) fn ask(
    protos: &[Proto],
    units: &[Unit],
    alike: &[(usize, usize, String)],
    changed: &dyn Fn(usize, usize) -> Vec<String>,
) -> Vec<Question> {
    let mut out: Vec<Question> = Vec::new();
    let mut push = |mut members: Vec<usize>, mut facts: Vec<String>, suggested| {
        members.sort_by_key(|&m| units[m].newest());
        // Главная улика — что изменилось, словами.
        for pair in members.windows(2) {
            let lines = changed(pair[0], pair[1]);
            if !lines.is_empty() {
                facts.push(format!("что изменилось: {}", lines.join("; ")));
            }
        }
        // В `id` имена по алфавиту: он не должен зависеть от времени файлов,
        // иначе один вопрос про два одновременных файла задали бы дважды.
        let mut ids: Vec<String> = members.iter().map(|&m| units[m].id()).collect();
        ids.sort();
        let names: Vec<String> = members
            .iter()
            .map(|&m| format!("«{}»", label(units, &members, m)))
            .collect();
        facts.extend(members.iter().map(|&m| {
            format!(
                "«{}» изменён {}",
                label(units, &members, m),
                date(units[m].newest())
            )
        }));
        let id = ids.join("|");
        if !out.iter().any(|q| q.id == id) {
            out.push(Question {
                id,
                text: format!("Это версии одной детали: {}?", names.join(" → ")),
                facts,
                suggested,
            });
        }
    };
    for p in protos.iter().filter(|p| p.name.is_some()) {
        if p.members.len() > 1 && p.guessed && p.shape != Some(ShapeVerdict::Confirmed) {
            let facts = if p.notes.is_empty() {
                vec!["версии угаданы только по именам файлов".to_string()]
            } else {
                p.notes.clone()
            };
            push(p.members.clone(), facts, None);
        }
        if let Some(rival) = p.rival {
            push(vec![rival, p.members[0]], p.notes.clone(), Some(false));
        }
    }
    // Похожие формой: спрашиваем, только когда у обеих деталей пара одна.
    let pairs_of = |p: usize| alike.iter().filter(|(a, b, _)| *a == p || *b == p).count();
    for (a, b, how) in alike {
        let (x, y) = (&protos[*a], &protos[*b]);
        if x.name.is_none() || y.name.is_none() || pairs_of(*a) != 1 || pairs_of(*b) != 1 {
            continue;
        }
        if let (Some(&m), Some(&n)) = (x.members.last(), y.members.last()) {
            let fact = format!("имена разные, а форма похожа: {how}");
            push(vec![m, n], vec![fact], None);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_covers_every_pair() {
        let said = Decisions::new(&[("вал|вал_v2|вал_v3".into(), false), ("а|Б".into(), true)]);
        assert_eq!(said.get("вал_v3", "вал"), Some(false));
        assert_eq!(said.get("вал_v2", "вал_v3"), Some(false));
        assert_eq!(said.get("б", "а"), Some(true), "регистр и порядок не важны");
        assert_eq!(said.get("вал", "а"), None);
    }

    #[test]
    fn dates_read_like_dates() {
        let at = |days: u64| UNIX_EPOCH + std::time::Duration::from_secs(days * 86_400 + 3600);
        assert_eq!(date(at(0)), "01.01.1970");
        assert_eq!(date(at(19_788)), "06.03.2024");
        assert_eq!(date(at(19_782)), "29.02.2024");
    }
}
