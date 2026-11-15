//! Замысел конструктора, прочитанный из родного файла CAD: список операций
//! с размерами, конфигурации, материал, свойства и признаки «спорной»
//! детали. Этими данными ядро обменивается с модулями точки
//! [`DesignSource`](crate::extension::DesignSource).
//!
//! Это не схема файла проекта: в `tree.yaml` и `meta.yaml` ничего отсюда
//! не пишется, поэтому миграций здесь нет. От [`FeatureTree`] (дерево от
//! аддина CAD) замысел из файла отличается тем, чего в файле нет: порядок
//! операций — не порядок дерева, размеры — как показаны на экране
//! (округлённые), погашение неизвестно.
//!
//! Здесь же — третий исход чтения ([`Partial`]): не «всё или ошибка», а
//! «прочитано вот это, а вот это — нет» ([`Gap`]).
//!
//! Файлы: здесь — данные, `diff.rs` — сравнение двух замыслов.
//!
//! [`FeatureTree`]: crate::tree::FeatureTree

mod diff;

use std::collections::BTreeMap;

pub use diff::{diff, Change};

/// Пропуск: что не удалось прочитать.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    /// Постоянный код для программ: `mesh.open`, `tree.garbled`.
    pub code: String,
    /// Слова для человека: что не прочитано и почему.
    pub note: String,
}

impl Gap {
    /// Пропуск с кодом и словами.
    pub fn new(code: impl Into<String>, note: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            note: note.into(),
        }
    }
}

/// Прочитанное и список того, что прочитать не удалось. Пустой список —
/// прочитано всё, что модуль умеет читать.
#[derive(Debug, Clone, PartialEq)]
pub struct Partial<T> {
    /// Что прочитано.
    pub value: T,
    /// Что не прочитано.
    pub gaps: Vec<Gap>,
}

impl<T> Partial<T> {
    /// Прочитано целиком.
    pub fn whole(value: T) -> Self {
        Self {
            value,
            gaps: Vec::new(),
        }
    }

    /// Пропусков нет.
    pub fn is_whole(&self) -> bool {
        self.gaps.is_empty()
    }
}

/// Размер операции, как он записан в файле.
#[derive(Debug, Clone, PartialEq)]
pub struct Size {
    /// Как показано на экране: `8`, `R2`, `45°`.
    pub shown: String,
    /// Число без приставок, если оно читается. Единицы — документа.
    pub value: Option<f64>,
}

/// Операция построения.
#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    /// Постоянный идентификатор из CAD: не меняется при переименовании.
    pub id: String,
    /// Тип, как его называет CAD (может быть на языке CAD).
    pub kind: String,
    /// Имя в дереве: «Скругление20».
    pub name: String,
    /// Служебный узел (папка, свет, плоскость по умолчанию): в словах об
    /// изменениях такие обычно не нужны.
    pub service: bool,
    /// Размеры по имени: `D1` → `8`.
    pub sizes: BTreeMap<String, Size>,
}

/// Причина не верить форме и числам, сохранённым в файле.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doubt {
    /// Постоянный код для программ: `needs_rebuild`, `derived`.
    pub code: String,
    /// Слова для человека.
    pub note: String,
}

/// Замысел, прочитанный из файла.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Design {
    /// Операции. Идентификаторы непустые и не повторяются. Порядок —
    /// какой смог дать модуль; на порядок дерева полагаться нельзя.
    pub operations: Vec<Operation>,
    /// Имена конфигураций.
    pub configurations: Vec<String>,
    /// Активная конфигурация: размеры и масса — её.
    pub active_configuration: Option<String>,
    /// Материал активной конфигурации.
    pub material: Option<String>,
    /// Масса, кг, как её сохранил CAD (могла устареть).
    pub saved_mass_kg: Option<f64>,
    /// Свойства, заполненные человеком (графы ЕСКД и прочие).
    pub properties: BTreeMap<String, String>,
    /// Признаки «спорной» детали. Пусто — причин сомневаться не видно.
    pub doubts: Vec<Doubt>,
}

impl Design {
    /// Операция по постоянному идентификатору.
    pub fn operation(&self, id: &str) -> Option<&Operation> {
        self.operations.iter().find(|o| o.id == id)
    }
}
