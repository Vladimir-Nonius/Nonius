//! Манифест документа: секция `swXmlContents/Features` у детали и
//! `swXmlContents/COMPINSTANCETREE` у сборки — один и тот же корень
//! `swSolidWorks` (`docs/formats/sldprt-design.md` §2.1, §4.1, §5.1).
//!
//! Отсюда: версия формата `swVersion`, конфигурации и какая из них
//! активная, «не перестроена», документы внешних ссылок и операции,
//! которые на них опираются, а у сборки — состав.
//!
//! Пути в манифесте — те, что были при сохранении на машине автора:
//! файла по ним может уже не быть.

use std::collections::HashMap;

use crate::xml::Node;

/// Документ, названный в манифесте: сам файл (первый) или тот, на который
/// он ссылается.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// `PART`, `ASSEMBLY`…
    pub kind: String,
    /// Время создания, секунды Unix. Совпадает с номером корня `KeyWords`.
    pub created: Option<i64>,
    /// Путь при сохранении.
    pub path: String,
    /// Исходный файл импорта (`.stp`), если документ импортирован.
    pub imported_from: Option<String>,
}

/// Конфигурация документа.
#[derive(Debug, Clone, PartialEq)]
pub struct Configuration {
    /// Имя.
    pub name: String,
    /// Номер `N` секций `Config-N…`; он же `id` в `KeyWords`.
    pub id: Option<i64>,
    /// Активная при сохранении.
    pub active: bool,
    /// Не перестроена с последней правки.
    pub needs_update: bool,
}

/// Операция детали, которая опирается на другой документ.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalFeature {
    /// Имя операции.
    pub name: String,
    /// Её номер в `KeyWords`.
    pub id: Option<i64>,
    /// Вид: `ProfileFeature` — эскиз в контексте сборки, `MBimport` —
    /// импорт, `Stock` — деталь, производная от другой.
    pub kind: String,
    /// Путь документа, на который ссылается.
    pub document: Option<String>,
}

/// Экземпляр компонента в сборке.
#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    /// Имя экземпляра: `пин-2`.
    pub name: String,
    /// Номер экземпляра; он же `id` элемента `Reference` в `KeyWords`.
    pub id: Option<i64>,
    /// Конфигурация компонента.
    pub configuration: String,
    /// Путь файла компонента при сохранении.
    pub path: Option<String>,
    /// Погашен.
    pub suppressed: bool,
    /// Скрыт.
    pub hidden: bool,
    /// Виртуальный: лежит внутри файла сборки, отдельного файла нет.
    pub virtual_part: bool,
    /// Импортирован (из STEP): тоже лежит внутри сборки.
    pub imported: bool,
    /// Исключён из спецификации.
    pub out_of_bom: bool,
    /// Положение: матрица 4×4 по строкам, перенос — в метрах.
    pub transform: Option<[f64; 16]>,
    /// Насколько глубоко: 0 — прямо в этой сборке, 1 — в её подсборке…
    pub depth: usize,
}

/// Содержимое манифеста.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Manifest {
    /// Версия формата SolidWorks (9000, 16000, 18000…). Какому году она
    /// отвечает — неизвестно.
    pub sw_version: Option<i64>,
    /// Документы: первый — сам файл.
    pub documents: Vec<Document>,
    /// Конфигурации.
    pub configurations: Vec<Configuration>,
    /// Счётчик правок модели активной конфигурации.
    pub modified_stamp: Option<i64>,
    /// Операции с внешней ссылкой (у детали).
    pub external: Vec<ExternalFeature>,
    /// Состав (у сборки): все экземпляры по дереву, сверху вниз.
    pub components: Vec<Component>,
}

/// Подсборки в подсборках глубже не разворачиваем: защита от петли в
/// испорченном файле.
const MAX_DEPTH: usize = 32;
/// И больше экземпляров не разворачиваем.
const MAX_COMPONENTS: usize = 200_000;

struct Model<'a> {
    node: &'a Node,
    path: Option<&'a str>,
    imported: bool,
}

fn transform(text: &str) -> Option<[f64; 16]> {
    let numbers: Vec<f64> = text
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .filter(|n: &f64| n.is_finite())
        .collect();
    numbers.try_into().ok()
}

impl Manifest {
    /// Разобрать корень `swSolidWorks`. `None` — корень другой.
    pub fn read(root: &Node) -> Option<Self> {
        if root.local() != "swSolidWorks" {
            return None;
        }
        let list = |name: &'static str, item: &'static str| {
            root.child(name)
                .into_iter()
                .flat_map(move |l| l.named(item))
        };
        let files: HashMap<i64, &Node> = list("swHeader", "swFile")
            .filter_map(|f| Some((f.number("id")?, f)))
            .collect();
        let models: HashMap<i64, Model> = list("swModelList", "swModel")
            .filter_map(|node| {
                let file = node.number("swFileRef").and_then(|id| files.get(&id));
                let model = Model {
                    node,
                    path: file.and_then(|f| f.attr("swPath")),
                    imported: file.is_some_and(|f| f.attr("swImportedPath").is_some()),
                };
                Some((node.number("id")?, model))
            })
            .collect();
        let mut manifest = Self {
            sw_version: root.number("swVersion"),
            ..Self::default()
        };
        manifest.documents = list("swHeader", "swFile")
            .map(|f| Document {
                kind: f.attr("swDocType").unwrap_or("").to_string(),
                created: f.number("swCreationTime"),
                path: f.attr("swPath").unwrap_or("").to_string(),
                imported_from: f.attr("swImportedPath").map(str::to_string),
            })
            .collect();
        let mut top = None;
        for node in list("swConfigurationList", "swConfiguration") {
            let active = node.flag("swMostRecentConfiguration").unwrap_or(false);
            if active {
                top = node.number("swModelRef");
            }
            manifest.configurations.push(Configuration {
                name: node.attr("swName").unwrap_or("").to_string(),
                id: node.number("swID"),
                active,
                needs_update: node.flag("swConfigurationNeedsUpdate").unwrap_or(false),
            });
        }
        manifest.external = list("swExtFeatureList", "swExtFeature")
            .map(|node| ExternalFeature {
                name: node.attr("swName").unwrap_or("").to_string(),
                id: node.number("swID"),
                kind: node.attr("swFeatureType").unwrap_or("").to_string(),
                document: node
                    .number("swModelRef")
                    .and_then(|id| models.get(&id))
                    .and_then(|m| m.path)
                    .map(str::to_string),
            })
            .collect();
        if let Some(top) = top.and_then(|id| models.get(&id)) {
            manifest.modified_stamp = top.node.number("swLastModifiedStamp");
            manifest.unfold(top, &models, 0);
        }
        Some(manifest)
    }

    /// Развернуть состав модели: её экземпляры и, под каждым, экземпляры
    /// его подсборки.
    fn unfold(&mut self, model: &Model, models: &HashMap<i64, Model>, depth: usize) {
        for node in model.node.named("swReference") {
            if self.components.len() >= MAX_COMPONENTS {
                return;
            }
            let part = node.number("swModelRef").and_then(|id| models.get(&id));
            let name = node.attr("swName").unwrap_or("");
            let number = node.attr("swReferenceNumber").unwrap_or("");
            self.components.push(Component {
                name: format!("{name}-{number}"),
                id: node.number("swID"),
                configuration: node.attr("swConfigurationName").unwrap_or("").to_string(),
                path: part.and_then(|m| m.path).map(str::to_string),
                suppressed: node.flag("swSuppressed").unwrap_or(false),
                hidden: node.flag("swHidden").unwrap_or(false),
                virtual_part: node.flag("swIsVirtualComponent").unwrap_or(false),
                imported: part.is_some_and(|m| m.imported),
                out_of_bom: node.flag("swExcludeFromBOM").unwrap_or(false),
                transform: node.attr("swTransform").and_then(transform),
                depth,
            });
            if let Some(part) = part.filter(|_| depth < MAX_DEPTH) {
                self.unfold(part, models, depth + 1);
            }
        }
    }

    /// Активная конфигурация.
    pub fn active(&self) -> Option<&Configuration> {
        self.configurations.iter().find(|c| c.active)
    }

    /// Файлы, из которых сборка собрана напрямую: пути без повторов, в
    /// порядке файла. Виртуальные и импортированные компоненты не в счёт —
    /// они лежат внутри самой сборки.
    pub fn direct_files(&self) -> Vec<&str> {
        let mut seen = Vec::new();
        let own = self.components.iter().filter(|c| c.depth == 0);
        for path in own
            .filter(|c| !c.virtual_part && !c.imported)
            .filter_map(|c| c.path.as_deref())
        {
            if !seen.contains(&path) {
                seen.push(path);
            }
        }
        seen
    }
}
