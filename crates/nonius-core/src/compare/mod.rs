//! Отчёт об изменениях и встроенное сравнение паспортов версий (`meta.yaml`).
//!
//! Геометрическое сравнение с цветной подсветкой и сравнение дерева построения —
//! отдельные модули ([`crate::extension::Comparator`]). Здесь — общий формат
//! отчёта и простое сравнение контрольных чисел, которое работает всегда.
//!
//! Файлы: здесь — формат отчёта, `meta.rs` — встроенное сравнение паспортов,
//! `number.rs` — запись чисел без лишних нулей.

mod meta;
mod number;

use std::fmt;

use serde::{Deserialize, Serialize};

pub use meta::MetaComparator;

/// Параметры сравнения.
#[derive(Debug, Clone, PartialEq)]
pub struct CompareContext {
    /// Допуск по длинам, мм.
    pub tolerance_mm: f64,
    /// Относительный допуск для объёма, площади, массы.
    pub relative_tolerance: f64,
}

impl Default for CompareContext {
    fn default() -> Self {
        Self {
            tolerance_mm: 0.01,
            relative_tolerance: 1e-6,
        }
    }
}

/// Вид изменения. Цвет подсветки в интерфейсе привязан к виду.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Добавлено (зелёный).
    Added,
    /// Убрано (красный).
    Removed,
    /// Изменено значение.
    Modified,
    /// Сдвинуто без изменения формы (жёлтый).
    Moved,
    /// Переименовано.
    Renamed,
}

/// Одно изменение.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    /// Вид изменения.
    pub kind: ChangeKind,
    /// Что изменилось: «Масса», «D1@Эскиз2», «Фаска1».
    pub subject: String,
    /// Было.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Стало.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.before, &self.after) {
            (Some(b), Some(a)) => write!(f, "{}: {b} → {a}", self.subject),
            (None, Some(a)) => write!(f, "{}: добавлено ({a})", self.subject),
            (Some(b), None) => write!(f, "{}: убрано ({b})", self.subject),
            (None, None) => write!(f, "{}", self.subject),
        }
    }
}

/// Отчёт одного сравнения.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeReport {
    /// Какой модуль сравнивал.
    pub comparator: String,
    /// Изменения в стабильном порядке.
    pub changes: Vec<Change>,
}

impl ChangeReport {
    /// Пустой отчёт.
    pub fn empty(comparator: impl Into<String>) -> Self {
        Self {
            comparator: comparator.into(),
            changes: Vec::new(),
        }
    }

    /// Изменений нет.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}
