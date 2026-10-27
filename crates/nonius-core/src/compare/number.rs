//! Запись чисел в отчёте: без лишних нулей, размеры через «×».

use std::fmt;

/// Число без лишних нулей: 12.5, 0.003, 1200.
pub(super) struct Trim(pub(super) f64);

impl fmt::Display for Trim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = format!("{:.6}", self.0);
        let s = s.trim_end_matches('0').trim_end_matches('.');
        let s = if s == "-0" { "0" } else { s };
        if f.sign_plus() && !s.starts_with('-') {
            f.write_str("+")?;
        }
        f.write_str(s)
    }
}

pub(super) fn fmt_num(x: f64) -> String {
    Trim(x).to_string()
}

pub(super) fn fmt_size(s: [f64; 3]) -> String {
    format!("{} × {} × {}", Trim(s[0]), Trim(s[1]), Trim(s[2]))
}

pub(super) fn fmt_point(p: [f64; 3]) -> String {
    format!("({}, {}, {})", Trim(p[0]), Trim(p[1]), Trim(p[2]))
}
