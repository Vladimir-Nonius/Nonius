//! Отчёт об изменениях и встроенное сравнение паспортов версий (`meta.yaml`).
//!
//! Геометрическое сравнение с цветной подсветкой и сравнение дерева построения —
//! отдельные модули ([`crate::extension::Comparator`]). Здесь — общий формат
//! отчёта и простое сравнение контрольных чисел, которое работает всегда.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::extension::{Comparator, ExtensionPoint, Module, ModuleManifest, Snapshot, API_VERSION};
use crate::meta::GeometrySummary;

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

/// Встроенное сравнение паспортов: контрольные числа, габариты, материал,
/// свойства, версия CAD.
pub struct MetaComparator {
    manifest: ModuleManifest,
}

impl MetaComparator {
    /// Идентификатор модуля.
    pub const ID: &'static str = "meta-compare";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: ModuleManifest {
                id: Self::ID.into(),
                name: "Сравнение паспортов версий".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                api: API_VERSION,
                points: vec![ExtensionPoint::Comparator],
                permissions: vec![],
            },
        }
    }
}

impl Default for MetaComparator {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for MetaComparator {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl Comparator for MetaComparator {
    fn compare(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        ctx: &CompareContext,
    ) -> Result<ChangeReport> {
        let mut out = Vec::new();
        let (a, b) = (&before.meta, &after.meta);

        geometry_changes(&a.geometry, &b.geometry, ctx, &mut out);
        text_change(
            "Материал",
            a.material.as_deref(),
            b.material.as_deref(),
            &mut out,
        );
        text_change(
            "Версия CAD",
            a.source.cad_version.as_deref(),
            b.source.cad_version.as_deref(),
            &mut out,
        );

        let keys: BTreeSet<&String> = a.properties.keys().chain(b.properties.keys()).collect();
        for key in keys {
            text_change(
                &format!("Свойство «{key}»"),
                a.properties.get(key).map(String::as_str),
                b.properties.get(key).map(String::as_str),
                &mut out,
            );
        }

        Ok(ChangeReport {
            comparator: Self::ID.into(),
            changes: out,
        })
    }
}

fn geometry_changes(
    a: &GeometrySummary,
    b: &GeometrySummary,
    ctx: &CompareContext,
    out: &mut Vec<Change>,
) {
    scalar("Объём, мм³", a.volume_mm3, b.volume_mm3, ctx, out);
    scalar("Площадь, мм²", a.area_mm2, b.area_mm2, ctx, out);
    scalar("Масса, кг", a.mass_kg, b.mass_kg, ctx, out);

    match (a.bbox, b.bbox) {
        (Some(x), Some(y)) => {
            let (sx, sy) = (x.size(), y.size());
            let resized = (0..3).any(|i| (sx[i] - sy[i]).abs() > ctx.tolerance_mm);
            let moved = (0..3).any(|i| (x.min[i] - y.min[i]).abs() > ctx.tolerance_mm);
            if resized {
                out.push(Change {
                    kind: ChangeKind::Modified,
                    subject: "Габариты, мм".into(),
                    before: Some(fmt_size(sx)),
                    after: Some(fmt_size(sy)),
                });
            } else if moved {
                out.push(Change {
                    kind: ChangeKind::Moved,
                    subject: "Положение габаритов, мм".into(),
                    before: Some(fmt_point(x.min)),
                    after: Some(fmt_point(y.min)),
                });
            }
        }
        (x, y) => opt_change(
            "Габариты, мм",
            x.map(|b| fmt_size(b.size())),
            y.map(|b| fmt_size(b.size())),
            out,
        ),
    }

    count("Тела", a.bodies, b.bodies, out);
    count("Грани", a.faces, b.faces, out);
}

/// Числа считаются разными, если разница больше относительного допуска
/// (около нуля — от единицы, чтобы шум не давал изменений).
fn scalar(
    subject: &str,
    a: Option<f64>,
    b: Option<f64>,
    ctx: &CompareContext,
    out: &mut Vec<Change>,
) {
    match (a, b) {
        (Some(x), Some(y)) => {
            let diff = (x - y).abs();
            let scale = x.abs().max(y.abs());
            if diff > ctx.relative_tolerance * scale.max(1.0) {
                out.push(Change {
                    kind: ChangeKind::Modified,
                    subject: subject.into(),
                    before: Some(fmt_num(x)),
                    after: Some(format!("{} ({:+})", fmt_num(y), Trim(y - x))),
                });
            }
        }
        (x, y) => opt_change(subject, x.map(fmt_num), y.map(fmt_num), out),
    }
}

fn count(subject: &str, a: Option<u32>, b: Option<u32>, out: &mut Vec<Change>) {
    if a != b {
        opt_change(
            subject,
            a.map(|v| v.to_string()),
            b.map(|v| v.to_string()),
            out,
        );
    }
}

fn text_change(subject: &str, a: Option<&str>, b: Option<&str>, out: &mut Vec<Change>) {
    if a != b {
        opt_change(subject, a.map(str::to_owned), b.map(str::to_owned), out);
    }
}

fn opt_change(subject: &str, a: Option<String>, b: Option<String>, out: &mut Vec<Change>) {
    let kind = match (&a, &b) {
        (None, None) => return,
        (Some(x), Some(y)) if x == y => return,
        (None, Some(_)) => ChangeKind::Added,
        (Some(_), None) => ChangeKind::Removed,
        (Some(_), Some(_)) => ChangeKind::Modified,
    };
    out.push(Change {
        kind,
        subject: subject.into(),
        before: a,
        after: b,
    });
}

/// Число без лишних нулей: 12.5, 0.003, 1200.
struct Trim(f64);

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

fn fmt_num(x: f64) -> String {
    Trim(x).to_string()
}

fn fmt_size(s: [f64; 3]) -> String {
    format!("{} × {} × {}", Trim(s[0]), Trim(s[1]), Trim(s[2]))
}

fn fmt_point(p: [f64; 3]) -> String {
    format!("({}, {}, {})", Trim(p[0]), Trim(p[1]), Trim(p[2]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{BoundingBox, ItemKind, PartMeta};
    use crate::names::PartName;

    fn snap(meta: PartMeta) -> Snapshot {
        Snapshot {
            item: PartName::new("кронштейн").unwrap(),
            version: None,
            dir: ".".into(),
            meta,
            tree: None,
        }
    }

    fn base() -> PartMeta {
        let mut m = PartMeta::new(ItemKind::Part);
        m.geometry.volume_mm3 = Some(1000.0);
        m.geometry.mass_kg = Some(1.2);
        m.geometry.bbox = Some(BoundingBox {
            min: [0.0; 3],
            max: [40.0, 20.0, 5.0],
        });
        m.material = Some("Ст3".into());
        m
    }

    #[test]
    fn identical_versions_have_no_changes() {
        let r = MetaComparator::new()
            .compare(&snap(base()), &snap(base()), &CompareContext::default())
            .unwrap();
        assert!(r.is_empty(), "{:?}", r.changes);
    }

    #[test]
    fn noise_below_tolerance_is_ignored() {
        let mut b = base();
        b.geometry.volume_mm3 = Some(1000.0000001);
        b.geometry.bbox = Some(BoundingBox {
            min: [0.001, 0.0, 0.0],
            max: [40.001, 20.0, 5.0],
        });
        let r = MetaComparator::new()
            .compare(&snap(base()), &snap(b), &CompareContext::default())
            .unwrap();
        assert!(r.is_empty(), "{:?}", r.changes);
    }

    #[test]
    fn reports_mass_size_material_and_move() {
        let mut b = base();
        b.geometry.mass_kg = Some(1.32);
        b.material = Some("09Г2С".into());
        b.properties.insert("Обозначение".into(), "КР-01".into());
        let r = MetaComparator::new()
            .compare(&snap(base()), &snap(b.clone()), &CompareContext::default())
            .unwrap();
        let text: Vec<String> = r.changes.iter().map(ToString::to_string).collect();
        assert!(
            text.contains(&"Масса, кг: 1.2 → 1.32 (+0.12)".to_string()),
            "{text:?}"
        );
        assert!(
            text.contains(&"Материал: Ст3 → 09Г2С".to_string()),
            "{text:?}"
        );
        assert!(text
            .iter()
            .any(|t| t.starts_with("Свойство «Обозначение»: добавлено")));

        let mut moved = base();
        moved.geometry.bbox = Some(BoundingBox {
            min: [10.0, 0.0, 0.0],
            max: [50.0, 20.0, 5.0],
        });
        let r = MetaComparator::new()
            .compare(&snap(base()), &snap(moved), &CompareContext::default())
            .unwrap();
        assert_eq!(r.changes.len(), 1);
        assert_eq!(r.changes[0].kind, ChangeKind::Moved);
    }
}
