//! Числа для человека: разряды через пробел, без лишних знаков после запятой.

use nonius_core::compare::{Change, ChangeKind};

use crate::texts::texts;

/// Объём с пробелами между разрядами: «14 400 мм³».
pub fn volume_text(mm3: f64) -> String {
    format!("{} {}", group(&format!("{:.0}", mm3.max(0.0))), texts().mm3)
}

/// Строка изменения для панели «Числа»: значок и округлённые числа.
pub fn change_line(c: &Change) -> String {
    let mark = match c.kind {
        ChangeKind::Added => "+",
        ChangeKind::Removed => "−",
        _ => "~",
    };
    format!("{mark} {}", tidy(&c.to_string()))
}

/// Округлить дробные числа в тексте: большие — до десятых (16013.87614 →
/// 16013,9), малые — до трёх значащих (0.04213 → 0,0421). Целые не трогаются:
/// это может быть обозначение или год.
fn tidy(text: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
            i += 1;
        }
        let number: String = chars[start..i].iter().collect();
        let trailing_dot = number.ends_with('.');
        let digits = number.trim_end_matches('.');
        match digits.parse::<f64>() {
            Ok(v) if digits.contains('.') => {
                let places = match v.abs() {
                    x if x >= 100.0 => 1,
                    x if x >= 1.0 => 2,
                    x if x > 0.0 => (2 - x.log10().floor() as i32).clamp(3, 6) as usize,
                    _ => 0,
                };
                let rounded = format!("{v:.places$}");
                let rounded = if rounded.contains('.') {
                    rounded.trim_end_matches('0').trim_end_matches('.')
                } else {
                    &rounded
                };
                out.push_str(&rounded.replace('.', ","));
            }
            _ => out.push_str(digits),
        }
        if trailing_dot {
            out.push('.');
        }
    }
    out
}

/// Разряды целой части через неразрывный пробел.
fn group(digits: &str) -> String {
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('\u{a0}');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes_read_easily() {
        assert_eq!(volume_text(14400.4), "14\u{a0}400 мм³");
        assert_eq!(volume_text(402.1), "402 мм³");
        assert_eq!(volume_text(1234567.0), "1\u{a0}234\u{a0}567 мм³");
    }

    #[test]
    fn numbers_are_rounded() {
        let c = Change {
            kind: ChangeKind::Modified,
            subject: "Объём, мм³".into(),
            before: Some("14400".into()),
            after: Some("16013.87614 (+1613.87614)".into()),
        };
        assert_eq!(change_line(&c), "~ Объём, мм³: 14400 → 16013,9 (+1613,9)");
        assert_eq!(tidy("60 × 30 × 8. Год 2026"), "60 × 30 × 8. Год 2026");
        assert_eq!(
            tidy("0.5 и 2.04 и 0.04213 и 3.000"),
            "0,5 и 2,04 и 0,0421 и 3"
        );
    }
}
