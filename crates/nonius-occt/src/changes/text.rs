//! Запись размеров и простая векторная арифметика для описания изменений.

/// Размер без лишних нулей, до сотых: `8`, `0.5`, `12.35`. Мельче сотки
/// механику не нужно, а одинаковые размеры так пишутся одинаково.
pub(super) fn mm(value: f64) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

pub(super) type V = [f64; 3];

pub(super) fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(super) fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(super) fn scaled(a: V, k: f64) -> V {
    [a[0] * k, a[1] * k, a[2] * k]
}

pub(super) fn length(a: V) -> f64 {
    dot(a, a).sqrt()
}

/// Смотрят ли направления в одну сторону (до 0,01°).
pub(super) fn same_direction(a: V, b: V) -> bool {
    dot(a, b) > 1.0 - 1e-8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_have_no_trailing_zeros() {
        assert_eq!(mm(8.0), "8");
        assert_eq!(mm(0.5), "0.5");
        assert_eq!(mm(12.3454), "12.35");
        assert_eq!(mm(-0.0001), "0");
    }
}
