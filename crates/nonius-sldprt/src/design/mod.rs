//! Замысел конструктора прямо из файла SolidWorks, без SolidWorks:
//! список операций с размерами, конфигурации, свойства (масса, материал,
//! ЕСКД), состав сборки, модель чертежа, признаки «спорной» детали.
//!
//! Всё это лежит в файле обычным XML (`docs/formats/sldprt-design.md`).
//! Двоичные секции (точные значения размеров, эскизы, уравнения,
//! сопряжения) здесь не читаются.
//!
//! Чтение не бывает «всё или ничего»: что прочиталось — в [`Design`], а
//! чего прочитать не удалось — списком [`Design::gaps`]. Ошибка
//! ([`Unreadable`]) — только когда файл не открылся вовсе.
//!
//! Файлы: `keywords.rs` — операции, `manifest.rs` — конфигурации, внешние
//! ссылки и состав сборки, `props.rs` — свойства, `doubt.rs` — признаки
//! спорной детали.

mod doubt;
mod keywords;
mod manifest;
mod props;

use std::path::Path;

pub use doubt::Doubt;
pub use keywords::{Configuration as TreeConfiguration, Dimension, Mark, Operation, Tag, Tree};
pub use manifest::{Component, Configuration, Document, ExternalFeature, Manifest};
pub use props::{DrawingModel, MassProps, Property, Props};

use crate::container::{Container, Listing, Refusal, Section, Unreadable};
use crate::xml::{self, Node};

/// Вид документа — по тому, какие секции в нём есть.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
    /// Деталь.
    Part,
    /// Сборка.
    Assembly,
    /// Чертёж.
    Drawing,
}

/// Что именно не прочитано.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    /// Каталог секций: файл обрезан, секции найдены обходом.
    Catalog,
    /// Список операций (`swXmlContents/KeyWords`).
    Tree,
    /// Манифест: конфигурации, ссылки, состав сборки.
    Manifest,
    /// Свойства «по умолчанию»: единицы, сохранённая масса, модель чертежа.
    Information,
    /// Свойства документа.
    DocumentProps,
    /// Свойства конфигурации с этим номером.
    ConfigProps(i64),
}

/// Почему не прочитано.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Секции нет в файле.
    Missing,
    /// Секция испорчена: не распаковалась или не сошлась сумма.
    Damaged,
    /// Секция слишком велика для файла такого размера.
    OverBudget,
    /// В секции не XML.
    NotXml,
    /// XML оборван или перепутан: взято то, что разобралось.
    Garbled,
}

/// Пропуск: что не прочитано и почему.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gap {
    /// Что.
    pub piece: Piece,
    /// Почему.
    pub why: Why,
}

impl Gap {
    /// Постоянный код для программ: `tree.garbled`, `config_props.missing`.
    pub fn code(&self) -> String {
        let piece = match self.piece {
            Piece::Catalog => "catalog",
            Piece::Tree => "tree",
            Piece::Manifest => "manifest",
            Piece::Information => "information",
            Piece::DocumentProps => "document_props",
            Piece::ConfigProps(_) => "config_props",
        };
        let why = match self.why {
            Why::Missing => "missing",
            Why::Damaged => "damaged",
            Why::OverBudget => "over_budget",
            Why::NotXml => "not_xml",
            Why::Garbled => "garbled",
        };
        format!("{piece}.{why}")
    }
}

impl std::fmt::Display for Gap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.piece {
            Piece::Catalog => {
                return f
                    .write_str("файл обрезан: каталога секций нет, часть секций могла потеряться")
            }
            Piece::Tree => f.write_str("список операций")?,
            Piece::Manifest => f.write_str("конфигурации и ссылки на другие файлы")?,
            Piece::Information => f.write_str("единицы и сохранённая масса")?,
            Piece::DocumentProps => f.write_str("свойства документа")?,
            Piece::ConfigProps(n) => write!(f, "свойства конфигурации {n}")?,
        }
        f.write_str(match self.why {
            Why::Missing => ": в файле этого нет",
            Why::Damaged => ": секция испорчена",
            Why::OverBudget => ": секция слишком велика, не читаю",
            Why::NotXml => ": секция записана не так, как ожидалось",
            Why::Garbled => ": секция оборвана, прочитано не всё",
        })
    }
}

/// Замысел, прочитанный из файла.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Design {
    /// Вид документа, если он понятен.
    pub kind: Option<DocKind>,
    /// Список операций.
    pub tree: Option<Tree>,
    /// Конфигурации, внешние ссылки, состав сборки.
    pub manifest: Option<Manifest>,
    /// Свойства.
    pub props: Props,
    /// Что не прочитано. Пусто — прочитано всё, что мы умеем читать.
    pub gaps: Vec<Gap>,
}

/// Секция как XML; пропуски записываются в `gaps`.
fn section(
    file: &Container,
    name: &str,
    piece: Piece,
    expected: bool,
    gaps: &mut Vec<Gap>,
) -> Option<Node> {
    let mut gap = |why| gaps.push(Gap { piece, why });
    match file.section(name) {
        Section::Missing => {
            if expected {
                gap(Why::Missing);
            }
            None
        }
        Section::Refused(Refusal::Damaged) => {
            gap(Why::Damaged);
            None
        }
        Section::Refused(Refusal::OverBudget) => {
            gap(Why::OverBudget);
            None
        }
        Section::Data(raw) => match xml::parse(raw) {
            None => {
                gap(Why::NotXml);
                None
            }
            Some(doc) => {
                if !doc.clean {
                    gap(Why::Garbled);
                }
                Some(doc.root)
            }
        },
    }
}

impl Design {
    /// Прочитать замысел из файла на диске.
    pub fn open(path: &Path) -> Result<Self, Unreadable> {
        Ok(Self::read(&Container::open(path)?))
    }

    /// Прочитать замысел из открытого файла.
    pub fn read(file: &Container) -> Self {
        let mut design = Self::default();
        let mut gaps = Vec::new();
        if file.listing() == Listing::Scan {
            gaps.push(Gap {
                piece: Piece::Catalog,
                why: Why::Missing,
            });
        }
        let names = file.names();
        let has = |name: &str| names.iter().any(|n| n == name);
        let (part, assembly) = ("swXmlContents/Features", "swXmlContents/COMPINSTANCETREE");
        design.kind = if has(assembly) {
            Some(DocKind::Assembly)
        } else if has(part) {
            Some(DocKind::Part)
        } else if names.iter().any(|n| n.starts_with("DWG/")) || has("Contents/VBLists") {
            Some(DocKind::Drawing)
        } else {
            None
        };

        let tree = section(file, "swXmlContents/KeyWords", Piece::Tree, true, &mut gaps);
        design.tree = tree.as_ref().and_then(Tree::read);
        if tree.is_some() && design.tree.is_none() {
            gaps.push(Gap {
                piece: Piece::Tree,
                why: Why::NotXml,
            });
        }
        // У чертежа манифеста нет — это не пропуск.
        let manifest = match design.kind {
            Some(DocKind::Assembly) => Some(assembly),
            Some(DocKind::Part) => Some(part),
            _ => None,
        };
        if let Some(name) = manifest {
            let root = section(file, name, Piece::Manifest, true, &mut gaps);
            design.manifest = root.as_ref().and_then(Manifest::read);
            if root.is_some() && design.manifest.is_none() {
                gaps.push(Gap {
                    piece: Piece::Manifest,
                    why: Why::NotXml,
                });
            }
        }
        let information = "docProps/ISolidWorksInformation.xml";
        if let Some(root) = section(file, information, Piece::Information, true, &mut gaps) {
            design.props.read_information(&root);
        }
        let document = "docProps/custom.xml";
        if let Some(root) = section(file, document, Piece::DocumentProps, true, &mut gaps) {
            design.props.read_document(&root);
        }
        // Свойства конфигураций: всех, что названы в списке операций, и
        // тех, чьи секции есть в файле.
        let mut configs: Vec<i64> = names
            .iter()
            .filter_map(|n| {
                let number = n.strip_prefix("docProps/Config-")?;
                number.strip_suffix("-Properties.xml")?.parse().ok()
            })
            .collect();
        let named = design.tree.iter().flat_map(|t| &t.configurations);
        configs.extend(named.filter_map(|c| c.id));
        configs.sort_unstable();
        configs.dedup();
        for config in configs {
            let name = format!("docProps/Config-{config}-Properties.xml");
            let piece = Piece::ConfigProps(config);
            if let Some(root) = section(file, &name, piece, true, &mut gaps) {
                design.props.read_configuration(config, &root);
            }
        }
        design.gaps = gaps;
        design
    }

    /// Номер активной конфигурации: из манифеста, а без него — единственной
    /// конфигурации из списка операций.
    pub fn active_config(&self) -> Option<i64> {
        let from_manifest = self.manifest.as_ref().and_then(|m| m.active()?.id);
        from_manifest.or_else(|| match self.tree.as_ref()?.configurations.as_slice() {
            [only] => only.id,
            _ => None,
        })
    }

    /// Материал активной конфигурации.
    pub fn material(&self) -> Option<&str> {
        let active = self.active_config()?;
        let configs = self.tree.iter().flat_map(|t| &t.configurations);
        let from_tree = configs
            .filter(|c| c.id == Some(active))
            .find_map(|c| c.material.as_deref());
        let property = || Some(self.props.property(active, "Материал")?.shown.as_str());
        from_tree.or_else(property)
    }

    /// Масс-свойства активной конфигурации, как их сохранил SolidWorks.
    /// Могли устареть и обычно округлены до показа.
    pub fn saved_mass(&self) -> Option<MassProps> {
        self.props.mass.get(&self.active_config()?).copied()
    }

    /// Признаки «спорной» детали — см. [`Doubt`].
    pub fn doubts(&self) -> Vec<Doubt> {
        doubt::doubts(self)
    }
}
