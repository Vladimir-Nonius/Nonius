//! Признаки «спорной» детали: то, что видно в файле без SolidWorks и
//! говорит «сохранённому не верь, уточни» (`sldprt-design.md` §5.2).
//!
//! По этим признакам решают, будить ли SolidWorks ради точной формы.
//! Здесь только то, что читается из XML. Уравнения (они в двоичной секции)
//! не проверяются, устаревшую сетку показа проверяет `fingerprint`.

use super::{Design, DocKind};

/// Версии формата ниже этой читаются хуже: другой диалект.
const OLD_VERSION: i64 = 12_000;

/// Причина сомневаться в сохранённых в файле форме и числах.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Doubt {
    /// Активная конфигурация не перестроена после правки.
    NeedsRebuild,
    /// Деталь — производная от другой (операция `Stock`): форма зависит
    /// от чужого файла и может быть из нескольких тел.
    Derived,
    /// Столько операций опираются на другие файлы (эскиз в контексте
    /// сборки и подобное): те файлы могли измениться.
    ExternalReferences(usize),
    /// Импортированная геометрия: дерева и размеров нет, только форма.
    Imported,
    /// Размеры ведёт таблица параметров (Excel).
    DesignTable,
    /// Конфигураций несколько: размеры, форма и масса в файле — только
    /// активной.
    Configurations(usize),
    /// Старая версия формата: читается хуже.
    OldVersion(i64),
    /// Файл прочитан не целиком (см. `Design::gaps`).
    NotWhole,
}

impl Doubt {
    /// Постоянный код для программ.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NeedsRebuild => "needs_rebuild",
            Self::Derived => "derived",
            Self::ExternalReferences(_) => "external_references",
            Self::Imported => "imported",
            Self::DesignTable => "design_table",
            Self::Configurations(_) => "configurations",
            Self::OldVersion(_) => "old_version",
            Self::NotWhole => "not_whole",
        }
    }
}

impl std::fmt::Display for Doubt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsRebuild => f.write_str("модель не перестроена после последней правки"),
            Self::Derived => f.write_str("деталь сделана из другой детали (производная)"),
            Self::ExternalReferences(n) => {
                write!(f, "операций, которые опираются на другие файлы: {n}")
            }
            Self::Imported => f.write_str("геометрия импортирована: дерева построения нет"),
            Self::DesignTable => f.write_str("размеры ведёт таблица параметров"),
            Self::Configurations(n) => {
                write!(
                    f,
                    "конфигураций {n}: в файле форма и размеры только активной"
                )
            }
            Self::OldVersion(v) => write!(f, "старая версия формата SolidWorks ({v})"),
            Self::NotWhole => f.write_str("файл прочитан не целиком"),
        }
    }
}

/// Все признаки спорности, в постоянном порядке.
pub(super) fn doubts(design: &Design) -> Vec<Doubt> {
    let mut found = Vec::new();
    let manifest = design.manifest.as_ref();
    let external = manifest.map(|m| m.external.as_slice()).unwrap_or_default();
    if manifest.is_some_and(|m| m.active().is_some_and(|c| c.needs_update)) {
        found.push(Doubt::NeedsRebuild);
    }
    if external.iter().any(|e| e.kind == "Stock") {
        found.push(Doubt::Derived);
    }
    let linked = external
        .iter()
        .filter(|e| !matches!(e.kind.as_str(), "Stock" | "MBimport"))
        .count();
    if linked > 0 {
        found.push(Doubt::ExternalReferences(linked));
    }
    let operations = design.tree.iter().flat_map(|t| &t.operations);
    if external.iter().any(|e| e.kind == "MBimport")
        || operations
            .into_iter()
            .any(|o| o.kind == "Multi_CAD_Imported")
    {
        found.push(Doubt::Imported);
    }
    if design.props.design_table == Some(true) {
        found.push(Doubt::DesignTable);
    }
    // Конфигурации считаем у деталей: у сборки и чертежа «форма активной»
    // — не тот вопрос.
    let in_manifest = manifest.map_or(0, |m| m.configurations.len());
    let in_tree = design.tree.as_ref().map_or(0, |t| t.configurations.len());
    let configurations = in_manifest.max(in_tree);
    if design.kind == Some(DocKind::Part) && configurations > 1 {
        found.push(Doubt::Configurations(configurations));
    }
    if let Some(version) = manifest.and_then(|m| m.sw_version) {
        if version < OLD_VERSION {
            found.push(Doubt::OldVersion(version));
        }
    }
    if !design.gaps.is_empty() {
        found.push(Doubt::NotWhole);
    }
    found
}
