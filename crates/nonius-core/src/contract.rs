//! Контрактные тесты точек расширения.
//!
//! Каждый модуль прогоняет в своих тестах проверку своей точки:
//!
//! ```
//! use nonius_core::compare::MetaComparator;
//! use nonius_core::contract;
//! # use nonius_core::{extension::Snapshot, meta::{ItemKind, PartMeta}, PartName};
//! # let snap = |m| Snapshot { item: PartName::new("x").unwrap(), version: None, dir: ".".into(), meta: m, tree: None };
//! # let a = snap(PartMeta::new(ItemKind::Part));
//! # let mut m = PartMeta::new(ItemKind::Part); m.material = Some("Ст3".into());
//! # let b = snap(m);
//! contract::comparator(&MetaComparator::new(), &a, &b).unwrap();
//! ```
//!
//! Модуль, прошедший проверку, гарантированно совместим с ядром этой версии.
//! Функции возвращают список нарушений, чтобы автор модуля увидел все сразу.

use std::collections::HashSet;
use std::path::Path;

use crate::compare::CompareContext;
use crate::extension::{
    Comparator, Exporter, ExtensionPoint, GeometrySource, Module, Snapshot, TreeSource,
};
use crate::schema::Versioned;
use crate::{meta::PartMeta, tree::FeatureTree};

/// Нарушения контракта.
pub type Violations = Vec<String>;

fn finish(v: Violations) -> Result<(), Violations> {
    if v.is_empty() {
        Ok(())
    } else {
        Err(v)
    }
}

/// Манифест корректен и объявляет точку `point`.
pub fn manifest(module: &dyn Module, point: ExtensionPoint) -> Result<(), Violations> {
    let m = module.manifest();
    let mut v = m.problems();
    if !m.points.contains(&point) {
        v.push(format!("манифест не объявляет точку {point:?}"));
    }
    finish(v)
}

/// Источник геометрии: читает образец, результат корректен и повторяем,
/// чужие файлы не берёт.
pub fn geometry_source<S: GeometrySource>(src: &S, sample: &Path) -> Result<(), Violations> {
    let mut v = manifest(src, ExtensionPoint::GeometrySource)
        .err()
        .unwrap_or_default();
    if !src.can_read(sample) {
        v.push(format!("не берётся читать образец {}", sample.display()));
        return finish(v);
    }
    if src.can_read(Path::new("contract-check.nonius-unknown-format")) {
        v.push("берётся читать файл неизвестного формата".into());
    }
    match (src.read(sample), src.read(sample)) {
        (Ok(a), Ok(b)) => {
            if a != b {
                v.push("два чтения одного файла дали разный результат".into());
            }
            if a.schema != PartMeta::CURRENT {
                v.push(format!(
                    "схема meta {} вместо {}",
                    a.schema,
                    PartMeta::CURRENT
                ));
            }
            if !a.geometry.is_valid() {
                v.push(format!("некорректные контрольные числа: {:?}", a.geometry));
            }
        }
        (Err(e), _) | (_, Err(e)) => v.push(format!("ошибка чтения образца: {e}")),
    }
    finish(v)
}

/// Источник дерева: идентификаторы фич непустые и уникальные, родители
/// ссылаются на фичи выше по дереву, чтение повторяемо.
pub fn tree_source<S: TreeSource>(src: &S, sample: &Path) -> Result<(), Violations> {
    let mut v = manifest(src, ExtensionPoint::TreeSource)
        .err()
        .unwrap_or_default();
    if !src.can_read(sample) {
        v.push(format!("не берётся читать образец {}", sample.display()));
        return finish(v);
    }
    match (src.read_tree(sample), src.read_tree(sample)) {
        (Ok(a), Ok(b)) => {
            if a != b {
                v.push("два чтения одного файла дали разный результат".into());
            }
            v.extend(tree_problems(&a));
        }
        (Err(e), _) | (_, Err(e)) => v.push(format!("ошибка чтения образца: {e}")),
    }
    finish(v)
}

/// Проверить само дерево построения.
pub fn tree_problems(tree: &FeatureTree) -> Violations {
    let mut v = Vec::new();
    if tree.schema != FeatureTree::CURRENT {
        v.push(format!(
            "схема tree {} вместо {}",
            tree.schema,
            FeatureTree::CURRENT
        ));
    }
    let mut seen = HashSet::new();
    for f in &tree.features {
        if f.id.is_empty() {
            v.push(format!("у фичи «{}» пустой id", f.name));
        }
        for p in &f.parents {
            if !seen.contains(p.as_str()) {
                v.push(format!(
                    "фича «{}»: родитель {p} не найден выше по дереву",
                    f.name
                ));
            }
        }
        if !seen.insert(f.id.as_str()) {
            v.push(format!("id {} повторяется", f.id));
        }
    }
    v
}

/// Сравнение: версия сама с собой даёт пустой отчёт, результат повторяем,
/// «есть ли изменения» не зависит от направления.
pub fn comparator<C: Comparator>(cmp: &C, a: &Snapshot, b: &Snapshot) -> Result<(), Violations> {
    let mut v = manifest(cmp, ExtensionPoint::Comparator)
        .err()
        .unwrap_or_default();
    let ctx = CompareContext::default();
    let run = |x: &Snapshot, y: &Snapshot| cmp.compare(x, y, &ctx);
    match (run(a, a), run(a, b), run(a, b), run(b, a)) {
        (Ok(same), Ok(ab1), Ok(ab2), Ok(ba)) => {
            if !same.is_empty() {
                v.push(format!(
                    "версия сама с собой даёт изменения: {:?}",
                    same.changes
                ));
            }
            if ab1 != ab2 {
                v.push("два сравнения одних версий дали разный результат".into());
            }
            if ab1.is_empty() != ba.is_empty() {
                v.push("наличие изменений зависит от направления сравнения".into());
            }
            if ab1.comparator != cmp.manifest().id {
                v.push(format!(
                    "отчёт подписан «{}», а не id модуля",
                    ab1.comparator
                ));
            }
        }
        (Err(e), ..) | (_, Err(e), ..) | (.., Err(e), _) | (.., Err(e)) => {
            v.push(format!("ошибка сравнения: {e}"))
        }
    }
    finish(v)
}

/// Экспорт: объявлен хотя бы один формат, каждый создаёт файл внутри `out_dir`.
pub fn exporter<E: Exporter>(
    exp: &E,
    snapshot: &Snapshot,
    out_dir: &Path,
) -> Result<(), Violations> {
    let mut v = manifest(exp, ExtensionPoint::Exporter)
        .err()
        .unwrap_or_default();
    let formats = exp.formats();
    if formats.is_empty() {
        v.push("не объявлено ни одного формата".into());
    }
    for f in formats {
        match exp.export(snapshot, &f, out_dir) {
            Ok(path) if !path.starts_with(out_dir) => {
                v.push(format!("{f}: файл {} вне папки выгрузки", path.display()))
            }
            Ok(path) if !path.is_file() => {
                v.push(format!("{f}: файл {} не создан", path.display()))
            }
            Ok(_) => {}
            Err(e) => v.push(format!("{f}: {e}")),
        }
    }
    finish(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::MetaComparator;
    use crate::meta::{ItemKind, PartMeta};
    use crate::tree::Feature;
    use crate::PartName;

    fn snap(meta: PartMeta) -> Snapshot {
        Snapshot {
            item: PartName::new("вал").unwrap(),
            version: None,
            dir: ".".into(),
            meta,
            tree: None,
        }
    }

    #[test]
    fn builtin_comparator_passes_contract() {
        let a = snap(PartMeta::new(ItemKind::Part));
        let mut m = PartMeta::new(ItemKind::Part);
        m.geometry.mass_kg = Some(2.0);
        contract_ok(comparator(&MetaComparator::new(), &a, &snap(m)));
    }

    #[test]
    fn tree_problems_are_found() {
        let f = |id: &str, parents: &[&str]| Feature {
            id: id.into(),
            kind: "Extrusion".into(),
            name: format!("Фича {id}"),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            suppressed: false,
            dimensions: Default::default(),
        };
        let mut t = FeatureTree::new();
        t.features = vec![f("1", &[]), f("2", &["1"])];
        assert!(tree_problems(&t).is_empty());
        t.features = vec![f("1", &["2"]), f("2", &[]), f("2", &[])];
        assert_eq!(tree_problems(&t).len(), 2);
    }

    fn contract_ok(r: Result<(), Violations>) {
        if let Err(v) = r {
            panic!("нарушения контракта:\n- {}", v.join("\n- "));
        }
    }
}
