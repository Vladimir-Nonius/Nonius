//! `tree.yaml` — текстовый слепок дерева построения.
//!
//! Пишется источником дерева (аддин SolidWorks, позже FreeCAD). Фичи
//! сопоставляются между версиями по [`Feature::id`] (persistent ID из CAD),
//! поэтому переименование не выглядит как удаление + добавление.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::schema::Versioned;

/// Размер или параметр фичи.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    /// Значение.
    pub value: f64,
    /// Единицы: `mm`, `deg`, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// Фича дерева построения.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    /// Постоянный идентификатор из CAD. Не меняется при переименовании.
    pub id: String,
    /// Тип фичи: `Extrusion`, `Chamfer`, `Sketch`, …
    pub kind: String,
    /// Имя в дереве: «Бобышка-Вытянуть1».
    pub name: String,
    /// Идентификаторы родительских фич.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<String>,
    /// Подавлена ли фича.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub suppressed: bool,
    /// Размеры по имени: `D1` → 40 мм.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dimensions: BTreeMap<String, Dimension>,
}

/// Содержимое `tree.yaml`. Фичи идут в порядке дерева.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureTree {
    /// Версия схемы файла.
    pub schema: u32,
    /// Фичи в порядке дерева.
    #[serde(default)]
    pub features: Vec<Feature>,
    /// Глобальные переменные и уравнения: имя → выражение.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub equations: BTreeMap<String, String>,
    /// Имена конфигураций.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub configurations: Vec<String>,
}

impl FeatureTree {
    /// Пустое дерево текущей схемы.
    pub fn new() -> Self {
        Self {
            schema: Self::CURRENT,
            features: Vec::new(),
            equations: BTreeMap::new(),
            configurations: Vec::new(),
        }
    }

    /// Найти фичу по постоянному идентификатору.
    pub fn feature(&self, id: &str) -> Option<&Feature> {
        self.features.iter().find(|f| f.id == id)
    }
}

impl Default for FeatureTree {
    fn default() -> Self {
        Self::new()
    }
}

impl Versioned for FeatureTree {
    const KIND: &'static str = "tree";
    const CURRENT: u32 = 1;
}
