//! `meta.yaml` — паспорт версии детали: откуда она, масса, объём, габариты,
//! материал, свойства. Файл маленький и всегда хранится локально, поэтому
//! историю можно смотреть, не скачивая сами модели.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::schema::Versioned;

/// Деталь или сборка.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// Деталь.
    #[default]
    Part,
    /// Сборка.
    Assembly,
}

/// Откуда пришла версия.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SourceInfo {
    /// CAD-система: `solidworks`, `kompas`, `freecad`, `step` (прислали STEP).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cad: Option<String>,
    /// Версия CAD, в которой сохранён исходник (важно: новый SLDPRT не откроется в старом SolidWorks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cad_version: Option<String>,
    /// Модуль-источник, который сделал эту версию.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
}

/// Габаритный параллелепипед, мм.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    /// Минимальный угол (x, y, z).
    pub min: [f64; 3],
    /// Максимальный угол (x, y, z).
    pub max: [f64; 3],
}

impl BoundingBox {
    /// Размеры по осям.
    pub fn size(&self) -> [f64; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }

    /// Углы упорядочены и все числа конечные.
    pub fn is_valid(&self) -> bool {
        (0..3).all(|i| {
            self.min[i].is_finite() && self.max[i].is_finite() && self.min[i] <= self.max[i]
        })
    }
}

/// Контрольные числа геометрии. Единицы: мм, мм², мм³, кг.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GeometrySummary {
    /// Объём, мм³.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_mm3: Option<f64>,
    /// Площадь поверхности, мм².
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_mm2: Option<f64>,
    /// Масса, кг (если известен материал).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass_kg: Option<f64>,
    /// Центр масс, мм.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center_of_mass: Option<[f64; 3]>,
    /// Габариты.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BoundingBox>,
    /// Число твёрдых тел.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bodies: Option<u32>,
    /// Число граней.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faces: Option<u32>,
}

impl GeometrySummary {
    /// Все заданные числа конечные и неотрицательные, габариты упорядочены.
    pub fn is_valid(&self) -> bool {
        let non_negative = |v: Option<f64>| v.is_none_or(|x| x.is_finite() && x >= 0.0);
        non_negative(self.volume_mm3)
            && non_negative(self.area_mm2)
            && non_negative(self.mass_kg)
            && self
                .center_of_mass
                .is_none_or(|c| c.iter().all(|x| x.is_finite()))
            && self.bbox.is_none_or(|b| b.is_valid())
    }
}

/// Содержимое `meta.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartMeta {
    /// Версия схемы файла.
    pub schema: u32,
    /// Деталь или сборка.
    #[serde(default)]
    pub kind: ItemKind,
    /// Откуда пришла версия.
    #[serde(default)]
    pub source: SourceInfo,
    /// Материал.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// Плотность материала, кг/м³. По ней считается масса; без материала
    /// не задаётся — плотность «по умолчанию» молча не подставляется.
    /// Поле добавлено без смены схемы: старые файлы читаются как есть.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density_kg_m3: Option<f64>,
    /// Контрольные числа геометрии.
    #[serde(default)]
    pub geometry: GeometrySummary,
    /// Пользовательские свойства (из SolidWorks custom properties и т.п.).
    /// `BTreeMap` — чтобы порядок в файле был стабильным.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,
}

impl PartMeta {
    /// Пустой паспорт текущей схемы.
    pub fn new(kind: ItemKind) -> Self {
        Self {
            schema: Self::CURRENT,
            kind,
            source: SourceInfo::default(),
            material: None,
            density_kg_m3: None,
            geometry: GeometrySummary::default(),
            properties: BTreeMap::new(),
        }
    }
}

impl Versioned for PartMeta {
    const KIND: &'static str = "meta";
    const CURRENT: u32 = 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use std::path::Path;

    #[test]
    fn meta_round_trips_and_is_stable() {
        let mut m = PartMeta::new(ItemKind::Part);
        m.material = Some("Сталь 09Г2С".into());
        m.geometry.volume_mm3 = Some(1250.5);
        m.geometry.bbox = Some(BoundingBox {
            min: [0.0, 0.0, 0.0],
            max: [40.0, 20.0, 5.0],
        });
        m.properties.insert("Обозначение".into(), "КР-01".into());
        m.properties.insert("Автор".into(), "Владимир".into());

        let p = Path::new("meta.yaml");
        let a = schema::to_string(&m, p).unwrap();
        let back: PartMeta = schema::from_str(&a, p).unwrap();
        assert_eq!(back, m);
        assert_eq!(
            schema::to_string(&back, p).unwrap(),
            a,
            "запись должна быть стабильной"
        );
        assert!(a.starts_with("schema: 1\n"));
    }

    #[test]
    fn file_without_density_still_reads() {
        // meta.yaml, записанный до появления плотности.
        let old = "schema: 1\nkind: part\nmaterial: Ст3\ngeometry:\n  volume_mm3: 1000.0\n";
        let m: PartMeta = schema::from_str(old, Path::new("meta.yaml")).unwrap();
        assert_eq!(m.material.as_deref(), Some("Ст3"));
        assert_eq!(m.density_kg_m3, None);
        let text = schema::to_string(&m, Path::new("meta.yaml")).unwrap();
        assert!(!text.contains("density"), "пустое поле не пишется: {text}");
    }

    #[test]
    fn newer_schema_is_refused() {
        let err = schema::from_str::<PartMeta>("schema: 99\n", Path::new("meta.yaml")).unwrap_err();
        assert!(err.to_string().contains("обновите Nonius"), "{err}");
        assert!(schema::from_str::<PartMeta>("kind: part\n", Path::new("m")).is_err());
    }

    #[test]
    fn summary_validation() {
        let mut g = GeometrySummary::default();
        assert!(g.is_valid());
        g.volume_mm3 = Some(-1.0);
        assert!(!g.is_valid());
        g.volume_mm3 = Some(1.0);
        g.bbox = Some(BoundingBox {
            min: [1.0, 0.0, 0.0],
            max: [0.0, 1.0, 1.0],
        });
        assert!(!g.is_valid());
    }
}
