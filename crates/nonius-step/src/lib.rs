//! Чтение заголовка STEP (ISO 10303-21) без геометрического ядра.
//!
//! Заголовок говорит, какая CAD-система сделала файл, кто автор и по какой
//! схеме записана геометрия (AP203/AP214/AP242). Этого хватает, чтобы сразу
//! заполнить `meta.yaml` при импорте. Контрольные числа (объём, площадь,
//! габариты) посчитает модуль на OpenCascade — он подключится к той же точке
//! расширения и заменит этот.
//!
//! Файлы: здесь — модуль `step-header`, `header.rs` — разбор заголовка.

mod header;

use std::path::Path;

use nonius_core::extension::{ExtensionPoint, GeometrySource, Module, ModuleManifest, API_VERSION};
use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::Result;

pub use header::StepHeader;

/// Модуль: источник геометрии по заголовку STEP.
pub struct StepHeaderSource {
    manifest: ModuleManifest,
}

impl StepHeaderSource {
    /// Идентификатор модуля.
    pub const ID: &'static str = "step-header";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: ModuleManifest {
                id: Self::ID.into(),
                name: "Заголовок STEP".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                api: API_VERSION,
                points: vec![ExtensionPoint::GeometrySource],
                permissions: vec![],
            },
        }
    }
}

impl Default for StepHeaderSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for StepHeaderSource {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl GeometrySource for StepHeaderSource {
    fn can_read(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("step") || e.eq_ignore_ascii_case("stp"))
    }

    fn read(&self, path: &Path) -> Result<PartMeta> {
        let h = StepHeader::read(path)?;
        let mut meta = PartMeta::new(ItemKind::Part);
        let system = [&h.originating_system, &h.preprocessor]
            .into_iter()
            .map(|s| s.trim())
            .find(|s| !s.is_empty());
        meta.source.cad = Some(system.unwrap_or("STEP").to_string());
        meta.source.module = Some(Self::ID.into());
        if let Some(p) = h.protocol() {
            meta.properties.insert("STEP: протокол".into(), p.into());
        }
        if !h.authors.is_empty() {
            meta.properties
                .insert("STEP: автор".into(), h.authors.join(", "));
        }
        if !h.organizations.is_empty() {
            meta.properties
                .insert("STEP: организация".into(), h.organizations.join(", "));
        }
        Ok(meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('SolidWorks 2020 - Part'),'2;1');\nFILE_NAME('\\X2\\043A0440043E043D0448044204350439043D\\X0\\.STEP','2026-10-26T12:00:00',('\\X2\\0412043B0430043404380441043B04300432\\X0\\'),('Workshop'),'SwSTEP 2.0','SolidWorks 2020','');\nFILE_SCHEMA (('AUTOMOTIVE_DESIGN { 1 0 10303 214 3 1 1 }'));\nENDSEC;\nDATA;\n#1=CARTESIAN_POINT('',(0.,0.,0.));\nENDSEC;\nEND-ISO-10303-21;\n";

    #[test]
    fn parses_header_with_cyrillic() {
        let h = StepHeader::parse(SAMPLE).unwrap();
        assert_eq!(h.name, "кронштейн.STEP");
        assert_eq!(h.authors, ["Владислав"]);
        assert_eq!(h.organizations, ["Workshop"]);
        assert_eq!(h.preprocessor, "SwSTEP 2.0");
        assert_eq!(h.originating_system, "SolidWorks 2020");
        assert_eq!(h.protocol(), Some("AP214"));
    }

    #[test]
    fn handles_quotes_empty_fields_and_garbage() {
        let h = StepHeader::parse(
            "ISO-10303-21;HEADER;FILE_NAME('it''s, ok','',(''),$,'','KOMPAS-3D v22','');ENDSEC;",
        )
        .unwrap();
        assert_eq!(h.name, "it's, ok");
        assert!(h.authors.is_empty());
        assert_eq!(h.originating_system, "KOMPAS-3D v22");
        assert_eq!(h.protocol(), None);
        assert!(StepHeader::parse("просто текст").is_none());
    }

    #[test]
    fn passes_contract() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("кронштейн.step");
        std::fs::write(&path, SAMPLE).unwrap();
        let src = StepHeaderSource::new();
        if let Err(v) = nonius_core::contract::geometry_source(&src, &path) {
            panic!("нарушения контракта:\n- {}", v.join("\n- "));
        }
        let meta = src.read(&path).unwrap();
        assert_eq!(meta.source.cad.as_deref(), Some("SolidWorks 2020"));
        assert_eq!(meta.properties["STEP: протокол"], "AP214");

        let bad = dir.path().join("пусто.step");
        std::fs::write(&bad, "не STEP").unwrap();
        assert!(src.read(&bad).is_err());
    }
}
