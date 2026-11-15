//! Сравнение двух замыслов: что добавлено, удалено, переименовано, какой
//! размер изменился.
//!
//! Операции сопоставляются по постоянному идентификатору, а не по имени и
//! не по порядку: переименование не выглядит как «удалили и добавили», а
//! перестановка в списке изменением не считается.
//!
//! Здесь только данные — слов для человека и рисунков нет. Чего сравнение
//! не видит, потому что этого нет в замысле из файла: перестановку
//! операций в дереве, погашение, изменения мельче точности показа размера,
//! правки эскиза без размеров.

use std::collections::HashMap;

use super::{Design, Operation, Size};

/// Одно изменение между двумя замыслами.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Операция появилась.
    Added {
        /// Идентификатор операции.
        id: String,
        /// Имя.
        name: String,
        /// Тип.
        kind: String,
        /// Служебный узел.
        service: bool,
    },
    /// Операция исчезла.
    Removed {
        /// Идентификатор операции.
        id: String,
        /// Имя.
        name: String,
        /// Тип.
        kind: String,
        /// Служебный узел.
        service: bool,
    },
    /// Операцию переименовали.
    Renamed {
        /// Идентификатор операции.
        id: String,
        /// Прежнее имя.
        before: String,
        /// Новое имя.
        after: String,
        /// Служебный узел.
        service: bool,
    },
    /// У операции изменился размер. `None` — размера не было или не стало.
    Resized {
        /// Идентификатор операции.
        id: String,
        /// Имя операции (новое, если её заодно переименовали).
        operation: String,
        /// Тип операции.
        kind: String,
        /// Имя размера: `D1`.
        size: String,
        /// Прежнее значение.
        before: Option<Size>,
        /// Новое значение.
        after: Option<Size>,
    },
}

impl Change {
    /// Изменение касается служебного узла (папки, света, плоскости по
    /// умолчанию): в словах для человека такие обычно не нужны.
    pub fn is_service(&self) -> bool {
        match self {
            Self::Added { service, .. }
            | Self::Removed { service, .. }
            | Self::Renamed { service, .. } => *service,
            Self::Resized { .. } => false,
        }
    }
}

/// Изменения от `before` к `after`: сначала по операциям новой версии в её
/// порядке (добавлена, переименована, размеры), затем удалённые — в
/// порядке старой. Одинаковые замыслы дают пустой список.
pub fn diff(before: &Design, after: &Design) -> Vec<Change> {
    let old: HashMap<&str, &Operation> = before
        .operations
        .iter()
        .map(|o| (o.id.as_str(), o))
        .collect();
    let new: HashMap<&str, &Operation> = after
        .operations
        .iter()
        .map(|o| (o.id.as_str(), o))
        .collect();
    let mut changes = Vec::new();
    for now in &after.operations {
        let Some(was) = old.get(now.id.as_str()) else {
            changes.push(Change::Added {
                id: now.id.clone(),
                name: now.name.clone(),
                kind: now.kind.clone(),
                service: now.service,
            });
            continue;
        };
        if was.name != now.name {
            changes.push(Change::Renamed {
                id: now.id.clone(),
                before: was.name.clone(),
                after: now.name.clone(),
                service: now.service && was.service,
            });
        }
        // Размеры — по имени; имена идут по алфавиту, сначала новой версии.
        let gone = was
            .sizes
            .keys()
            .filter(|name| !now.sizes.contains_key(*name));
        for name in now.sizes.keys().chain(gone) {
            let (old_size, new_size) = (was.sizes.get(name), now.sizes.get(name));
            let same = match (old_size, new_size) {
                (Some(a), Some(b)) => a.shown == b.shown,
                _ => false,
            };
            if !same {
                changes.push(Change::Resized {
                    id: now.id.clone(),
                    operation: now.name.clone(),
                    kind: now.kind.clone(),
                    size: name.clone(),
                    before: old_size.cloned(),
                    after: new_size.cloned(),
                });
            }
        }
    }
    for was in &before.operations {
        if !new.contains_key(was.id.as_str()) {
            changes.push(Change::Removed {
                id: was.id.clone(),
                name: was.name.clone(),
                kind: was.kind.clone(),
                service: was.service,
            });
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(id: &str, name: &str, sizes: &[(&str, &str)]) -> Operation {
        Operation {
            id: id.into(),
            kind: "Скругление".into(),
            name: name.into(),
            service: false,
            sizes: sizes
                .iter()
                .map(|(name, shown)| {
                    let size = Size {
                        shown: shown.to_string(),
                        value: shown.parse().ok(),
                    };
                    (name.to_string(), size)
                })
                .collect(),
        }
    }

    fn design(operations: Vec<Operation>) -> Design {
        Design {
            operations,
            ..Design::default()
        }
    }

    #[test]
    fn operations_are_matched_by_id() {
        let before = design(vec![
            op("53", "Бобышка1", &[("D1", "15")]),
            op("2127", "Скругление20", &[("D1", "8"), ("D2", "14.5")]),
            op("70", "Фаска1", &[("D1", "2"), ("D2", "45°")]),
            op("90", "Вырез1", &[]),
        ]);
        let after = design(vec![
            // Порядок другой — это не изменение.
            op("2127", "Скругление20", &[("D1", "12"), ("D2", "14.5")]),
            op("53", "Основание", &[("D1", "15")]),
            op("70", "Фаска1", &[("D1", "2"), ("D3", "1")]),
            op("2151", "Бобышка6", &[]),
        ]);
        assert_eq!(diff(&before, &before), []);
        let changes = diff(&before, &after);
        let size = |shown: &str| {
            Some(Size {
                shown: shown.into(),
                value: shown.parse().ok(),
            })
        };
        let resized = |id: &str, operation: &str, name: &str, a: Option<Size>, b: Option<Size>| {
            Change::Resized {
                id: id.into(),
                operation: operation.into(),
                kind: "Скругление".into(),
                size: name.into(),
                before: a,
                after: b,
            }
        };
        assert_eq!(
            changes,
            [
                resized("2127", "Скругление20", "D1", size("8"), size("12")),
                Change::Renamed {
                    id: "53".into(),
                    before: "Бобышка1".into(),
                    after: "Основание".into(),
                    service: false,
                },
                resized("70", "Фаска1", "D3", None, size("1")),
                resized("70", "Фаска1", "D2", size("45°"), None),
                Change::Added {
                    id: "2151".into(),
                    name: "Бобышка6".into(),
                    kind: "Скругление".into(),
                    service: false,
                },
                Change::Removed {
                    id: "90".into(),
                    name: "Вырез1".into(),
                    kind: "Скругление".into(),
                    service: false,
                },
            ]
        );
        // Обратное сравнение меняет добавленное и удалённое местами.
        let back = diff(&after, &before);
        assert!(matches!(&back[..], [.., Change::Removed { id, .. }] if id == "2151"));
        assert!(back
            .iter()
            .any(|c| matches!(c, Change::Added { id, .. } if id == "90")));
    }

    #[test]
    fn service_changes_are_marked() {
        let mut folder = op("38", "History", &[]);
        folder.service = true;
        let changes = diff(
            &design(vec![]),
            &design(vec![folder, op("5", "Вырез", &[])]),
        );
        let marks: Vec<bool> = changes.iter().map(Change::is_service).collect();
        assert_eq!(marks, [true, false]);
    }
}
