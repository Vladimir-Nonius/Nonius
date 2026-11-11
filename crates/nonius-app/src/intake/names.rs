//! Имена файлов, как их пишут люди: `вал_v2`, `вал (2)`, `вал final`,
//! `вал доработанный`. Отсюда — догадка «это версии одной детали».
//!
//! Осторожно с числами: `ЗАГЛУШКА12` — это двенадцатая заглушка, а не
//! версия. Версией считается число только с пометкой (`v2`, `rev 3`,
//! `версия 2`) или в скобках, как его дописывает Проводник: `(2)`.

/// Что в имени говорит о версии.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mark {
    /// Номер: `v2`, `(2)`, `rev 3`.
    Number(u32),
    /// Слово: «копия», «final», «новый», «доработанный».
    Word,
}

/// Имя без пометок версии и сама пометка.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Parsed {
    /// Имя детали, как в файле, но без хвоста с версией.
    pub base: String,
    /// Пометка версии, если была.
    pub mark: Option<Mark>,
}

/// Слова, которыми помечают «ещё одну версию».
const WORDS: &[&str] = &[
    "копия",
    "copy",
    "final",
    "финал",
    "new",
    "old",
    "новый",
    "новая",
    "новое",
    "старый",
    "старая",
    "старое",
    "доработанный",
    "доработаный",
    "доработанная",
    "доработаная",
    "исправленный",
    "исправленная",
    "испр",
];

/// Пометки перед номером версии.
const BEFORE_NUMBER: &[&str] = &["v", "в", "ver", "rev", "вер", "версия", "ревизия"];

fn is_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '_' | '-' | '.' | ',')
}

/// Разобрать имя файла без расширения.
pub(super) fn parse(stem: &str) -> Parsed {
    let mut rest: Vec<char> = stem.chars().collect();
    let mut mark = None;
    loop {
        while rest.last().is_some_and(|c| is_separator(*c)) {
            rest.pop();
        }
        let Some((cut, found)) = tail(&rest) else {
            break;
        };
        // Пометка есть, но кроме неё в имени ничего: «версия 2», «копия».
        if rest[..cut].iter().all(|c| is_separator(*c)) {
            break;
        }
        rest.truncate(cut);
        // Номер главнее слова: «вал final (2)» — вторая версия.
        mark = match (mark, found) {
            (Some(Mark::Number(n)), _) => Some(Mark::Number(n)),
            (_, m) => Some(m),
        };
    }
    let base: String = rest.into_iter().collect();
    if base.is_empty() {
        return Parsed {
            base: stem.to_string(),
            mark: None,
        };
    }
    Parsed { base, mark }
}

/// Пометка версии в конце имени: где она начинается и что значит.
fn tail(name: &[char]) -> Option<(usize, Mark)> {
    // «(2)» — так Проводник называет копию.
    if name.last() == Some(&')') {
        let open = name.iter().rposition(|c| *c == '(')?;
        let n = number(&name[open + 1..name.len() - 1])?;
        return Some((open, Mark::Number(n)));
    }
    let start = name
        .iter()
        .rposition(|c| is_separator(*c))
        .map_or(0, |i| i + 1);
    let token: String = name[start..].iter().collect::<String>().to_lowercase();
    if WORDS.contains(&token.as_str()) {
        return Some((start, Mark::Word));
    }
    // «v2», «rev3» одним словом.
    let digits_from = token.find(|c: char| c.is_ascii_digit())?;
    let (word, digits) = token.split_at(digits_from);
    let n: u32 = digits.parse().ok()?;
    if BEFORE_NUMBER.contains(&word) {
        return Some((start, Mark::Number(n)));
    }
    // «версия 2», «v 2» — номер отдельным словом после пометки.
    if word.is_empty() && start > 0 {
        let mut before = &name[..start];
        while before.last().is_some_and(|c| is_separator(*c)) {
            before = &before[..before.len() - 1];
        }
        let word_start = before
            .iter()
            .rposition(|c| is_separator(*c))
            .map_or(0, |i| i + 1);
        let word: String = before[word_start..]
            .iter()
            .collect::<String>()
            .to_lowercase();
        if BEFORE_NUMBER.contains(&word.as_str()) {
            return Some((word_start, Mark::Number(n)));
        }
    }
    None
}

fn number(chars: &[char]) -> Option<u32> {
    if chars.is_empty() || !chars.iter().all(char::is_ascii_digit) {
        return None;
    }
    chars.iter().collect::<String>().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(stem: &str) -> (String, Option<Mark>) {
        let parsed = parse(stem);
        (parsed.base, parsed.mark)
    }

    #[test]
    fn version_marks_are_found() {
        assert_eq!(p("Test V1"), ("Test".into(), Some(Mark::Number(1))));
        assert_eq!(p("вал_v2"), ("вал".into(), Some(Mark::Number(2))));
        assert_eq!(p("вал (3)"), ("вал".into(), Some(Mark::Number(3))));
        assert_eq!(p("вал rev 4"), ("вал".into(), Some(Mark::Number(4))));
        assert_eq!(p("вал версия 5"), ("вал".into(), Some(Mark::Number(5))));
        assert_eq!(p("вал - копия"), ("вал".into(), Some(Mark::Word)));
        assert_eq!(
            p("впуск турьбо доработаный"),
            ("впуск турьбо".into(), Some(Mark::Word))
        );
        // Несколько пометок подряд; номер главнее слова.
        assert_eq!(p("вал_final(2)"), ("вал".into(), Some(Mark::Number(2))));
        assert_eq!(p("вал_v2_final"), ("вал".into(), Some(Mark::Number(2))));
    }

    #[test]
    fn plain_numbers_are_part_of_the_name() {
        assert_eq!(p("ЗАГЛУШКА12"), ("ЗАГЛУШКА12".into(), None));
        assert_eq!(p("Сборка1"), ("Сборка1".into(), None));
        assert_eq!(p("болт М8 30"), ("болт М8 30".into(), None));
        assert_eq!(p("RAIL-00"), ("RAIL-00".into(), None));
    }

    #[test]
    fn a_mark_alone_is_a_name() {
        assert_eq!(p("версия 2"), ("версия 2".into(), None));
        assert_eq!(p("копия"), ("копия".into(), None));
        assert_eq!(p("v2"), ("v2".into(), None));
        assert_eq!(p(""), ("".into(), None));
    }
}
