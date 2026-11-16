//! Модуль `sldprt-design` (точка [`DesignSource`]): замысел из файла
//! SolidWorks в том виде, в каком его понимает ядро.
//!
//! Подробное чтение — в `design/`; здесь только перевод в общие данные
//! ядра ([`nonius_core::design::Design`]): номера операций становятся
//! строками, пропуски и признаки спорности получают код и слова.

use std::path::Path;

use nonius_core::design as core;
use nonius_core::extension::{DesignSource, ExtensionPoint, Module, ModuleManifest, API_VERSION};
use nonius_core::{Error, Result};

use crate::design::{Design, Operation, Tag};

/// Модуль: замысел из файла `.sldprt`, `.sldasm`, `.slddrw`.
pub struct SldprtDesign {
    manifest: ModuleManifest,
}

impl SldprtDesign {
    /// Идентификатор модуля.
    pub const ID: &'static str = "sldprt-design";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: ModuleManifest {
                id: Self::ID.into(),
                name: "Замысел из файла SolidWorks".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                api: API_VERSION,
                points: vec![ExtensionPoint::Design],
                permissions: vec![],
            },
        }
    }
}

impl Default for SldprtDesign {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for SldprtDesign {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

/// Тип операции: записанный в файле, а если его нет — по тегу.
fn kind(operation: &Operation) -> String {
    if !operation.kind.is_empty() {
        return operation.kind.clone();
    }
    match operation.tag {
        Tag::Extrusion => "Extrusion",
        Tag::Cut => "Cut",
        Tag::Sketch => "Sketch",
        Tag::Feature => "Feature",
        Tag::Reference => "Reference",
    }
    .to_string()
}

fn operation(found: &Operation) -> core::Operation {
    let mut sizes = std::collections::BTreeMap::new();
    for dimension in &found.dimensions {
        // Одноимённых размеров у операции быть не должно; если есть —
        // оставляем первый.
        sizes
            .entry(dimension.name.clone())
            .or_insert_with(|| core::Size {
                shown: dimension.shown.clone(),
                value: dimension.value,
            });
    }
    core::Operation {
        id: found.id.to_string(),
        kind: kind(found),
        name: found.name.clone(),
        service: found.service,
        sizes,
    }
}

/// Перевести прочитанное из файла в данные ядра.
pub fn to_core(design: &Design) -> core::Partial<core::Design> {
    let tree = design.tree.as_ref();
    let manifest = design.manifest.as_ref();
    let from_manifest = manifest.map(|m| m.configurations.iter().map(|c| c.name.clone()));
    let from_tree = tree.map(|t| t.configurations.iter().map(|c| c.name.clone()));
    let mut configurations: Vec<String> = match (from_manifest, from_tree) {
        (Some(names), _) => names.collect(),
        (None, Some(names)) => names.collect(),
        (None, None) => Vec::new(),
    };
    configurations.dedup();
    let active = design.active_config();
    let mut properties = std::collections::BTreeMap::new();
    let own = active.and_then(|n| design.props.configurations.get(&n));
    for (name, property) in design
        .props
        .document
        .iter()
        .chain(own.into_iter().flatten())
    {
        properties.insert(name.clone(), property.shown.clone());
    }
    let value = core::Design {
        operations: tree
            .map(|t| t.operations.iter().map(operation).collect())
            .unwrap_or_default(),
        active_configuration: manifest
            .and_then(|m| m.active())
            .map(|c| c.name.clone())
            .filter(|name| configurations.contains(name)),
        configurations,
        material: design.material().map(str::to_string),
        saved_mass_kg: design
            .saved_mass()
            .map(|m| m.mass_kg)
            .filter(|m| m.is_finite() && *m >= 0.0),
        properties,
        doubts: design
            .doubts()
            .iter()
            .map(|d| core::Doubt {
                code: d.code().to_string(),
                note: d.to_string(),
            })
            .collect(),
    };
    let gaps = design
        .gaps
        .iter()
        .map(|g| core::Gap::new(g.code(), g.to_string()))
        .collect();
    core::Partial { value, gaps }
}

impl DesignSource for SldprtDesign {
    fn can_read(&self, path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            ["sldprt", "sldasm", "slddrw"]
                .iter()
                .any(|known| e.eq_ignore_ascii_case(known))
        })
    }

    fn read_design(&self, path: &Path) -> Result<core::Partial<core::Design>> {
        let design = Design::open(path).map_err(|why| Error::Module {
            module: Self::ID.into(),
            message: why.to_string(),
        })?;
        Ok(to_core(&design))
    }
}
