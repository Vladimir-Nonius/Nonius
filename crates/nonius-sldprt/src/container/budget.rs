//! Предел распаковки: сколько байт можно получить из одного файла.
//!
//! Размеры секций записаны в самом файле, а файл чужой: человек приносит
//! его откуда угодно. Поэтому заявленному размеру верим только до потолка,
//! который растёт вместе с файлом (правило — `docs/formats/practices.md`
//! §3, пределы cadmpeg):
//!
//! - одна секция — не больше 16 МиБ + 256 × размер файла, и не больше 2 ГиБ;
//! - все секции вместе — не больше 16 МиБ + 1000 × размер файла, и не
//!   больше 8 ГиБ.
//!
//! Настоящим файлам этого хватает с большим запасом: deflate редко сжимает
//! сильнее чем в десятки раз.

use std::sync::atomic::{AtomicU64, Ordering};

const BASE: u64 = 16 << 20;
const MAX_ONE: u64 = 2 << 30;
const MAX_ALL: u64 = 8 << 30;

/// Счётчик распакованных байт одного файла.
#[derive(Debug)]
pub struct Budget {
    one: u64,
    all: u64,
    spent: AtomicU64,
}

impl Budget {
    /// Пределы для файла размером `file` байт.
    pub fn for_file(file: usize) -> Self {
        let file = file as u64;
        let grow = |times: u64, most: u64| {
            file.checked_mul(times)
                .and_then(|n| n.checked_add(BASE))
                .map_or(most, |n| n.min(most))
        };
        Self {
            one: grow(256, MAX_ONE),
            all: grow(1000, MAX_ALL),
            spent: AtomicU64::new(0),
        }
    }

    /// Можно ли распаковать ещё `size` байт. Если да — они записаны в счёт.
    pub fn take(&self, size: usize) -> bool {
        let size = size as u64;
        if size > self.one {
            return false;
        }
        let mut spent = self.spent.load(Ordering::Relaxed);
        loop {
            let Some(total) = spent.checked_add(size).filter(|t| *t <= self.all) else {
                return false;
            };
            let swap = self.spent.compare_exchange_weak(
                spent,
                total,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
            match swap {
                Ok(_) => return true,
                Err(now) => spent = now,
            }
        }
    }

    /// Сколько байт уже распаковано.
    pub fn spent(&self) -> u64 {
        self.spent.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_grow_with_the_file_and_stop_at_the_ceiling() {
        let small = Budget::for_file(1000);
        assert!(small.take(16 << 20));
        assert!(
            !small.take((16 << 20) + 256_001),
            "одна секция сверх предела"
        );
        // Вместе: 16 МиБ + 1 000 000 байт, 16 МиБ уже взяли.
        assert!(small.take(1_000_000));
        assert!(!small.take(1), "общий предел исчерпан");
        assert_eq!(small.spent(), (16 << 20) + 1_000_000);

        let huge = Budget::for_file(usize::MAX);
        assert!(huge.take(2 << 30));
        assert!(!huge.take((2 << 30) + 1));
    }
}
