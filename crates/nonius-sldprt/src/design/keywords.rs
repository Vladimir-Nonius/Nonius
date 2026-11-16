//! Список операций детали из секции `swXmlContents/KeyWords`.
//!
//! Что здесь лежит и чего нет (`docs/formats/sldprt-design.md` §1):
//! - у каждой операции постоянный номер `id`, имя, тип и размеры — как они
//!   показаны на экране (число в единицах документа, округлённое);
//! - порядок элементов в файле — сортировка по тегу и `id`, а не порядок
//!   дерева построения. Мы отдаём операции по возрастанию `id`: это
//!   примерно порядок создания, но не порядок после перетаскивания;
//! - погашена ли операция — в наших файлах не записано;
//! - `Type` и имена — на языке SolidWorks, английские слова вперемешку;
//! - размеры — только активной конфигурации.

use crate::xml::Node;

/// Вид элемента по его тегу в файле.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    /// Вытягивание (бобышка или вырез).
    Extrusion,
    /// Вырез.
    Cut,
    /// Эскиз.
    Sketch,
    /// Всё прочее: скругления, фаски, плоскости, папки, материал.
    Feature,
    /// Экземпляр компонента (в сборке и чертеже).
    Reference,
}

/// Что стоит перед числом размера.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Просто число: длина в единицах документа или количество.
    Plain,
    /// Радиус: `R2`.
    Radius,
    /// Диаметр: `<MOD-DIAM>42`.
    Diameter,
    /// Угол в градусах: `45°`.
    Angle,
    /// Текст, который числом не читается (дроби дюймов, форматированный).
    Text,
}

/// Размер операции.
#[derive(Debug, Clone, PartialEq)]
pub struct Dimension {
    /// Имя: `D1`, `D2`…
    pub name: String,
    /// Как записано в файле: `8`, `R2`, `45°`.
    pub shown: String,
    /// Число, если оно читается.
    pub value: Option<f64>,
    /// Радиус, диаметр, угол или просто число.
    pub mark: Mark,
}

/// Операция дерева построения.
#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    /// Постоянный номер операции в документе.
    pub id: i64,
    /// Тег элемента.
    pub tag: Tag,
    /// Имя в дереве: «Скругление20».
    pub name: String,
    /// Тип, как записан (на языке SolidWorks); пусто, если не записан.
    pub kind: String,
    /// Размеры в порядке файла.
    pub dimensions: Vec<Dimension>,
    /// Номера эскизов-детей (у вытягивания один ребёнок — его профиль).
    pub children: Vec<i64>,
    /// Служебный узел: папка, свет, плоскость по умолчанию, атрибут.
    pub service: bool,
}

/// Конфигурация, как она названа в `KeyWords`.
#[derive(Debug, Clone, PartialEq)]
pub struct Configuration {
    /// Номер `N` её секций `Config-N…`.
    pub id: Option<i64>,
    /// Имя.
    pub name: String,
    /// Материал конфигурации.
    pub material: Option<String>,
}

/// Содержимое `KeyWords`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tree {
    /// Номер корня: у детали равен времени создания файла (секунды Unix).
    pub root_id: Option<i64>,
    /// Имя корня — обозначение документа, а не имя файла.
    pub root_name: String,
    /// Все конфигурации документа.
    pub configurations: Vec<Configuration>,
    /// Операции по возрастанию `id`; узлы без своего номера (`-1`) сюда
    /// не попадают — они не уникальны и только служебные.
    pub operations: Vec<Operation>,
    /// Сколько узлов пропущено: без номера или с повторным номером.
    pub skipped: usize,
}

/// Типы служебных узлов: по-русски, как пишет русский SolidWorks, и
/// английские слова, которые встречаются вперемешку.
const SERVICE_KINDS: [&str; 46] = [
    "Примечания",
    "Твердые тела",
    "Тела поверхности",
    "Связь проектов",
    "Заметки",
    "Материал SOLIDWORKS",
    "Датчики",
    "Уравнения",
    "Виды с разнесенными частями",
    "Часто используемые",
    "История",
    "Наборы выбора",
    "Папка",
    "Папка ",
    "Исправления",
    "Таблицы",
    "Таблица конфигурации",
    "Источники света, камеры и сцены",
    "Рассеянный",
    "Направленный",
    "Исходная точка",
    "Оси",
    "Плоскости",
    "Эскизы",
    "Point",
    "Папка детали",
    "Sheet-Metal Master",
    "ConfigurationManager",
    "Annotations",
    "Solid Bodies",
    "Surface Bodies",
    "Design Binder",
    "Notes",
    "SOLIDWORKS Material",
    "Sensors",
    "Equations",
    "Favorites",
    "History",
    "Selection Sets",
    "Folder",
    "Markups",
    "Tables",
    "Lights, Cameras and Scene",
    "Ambient",
    "Directional",
    "Origin",
];

/// Имя — идентификатор вида `{XXXXXXXX-XXXX-…}`: так названы атрибуты.
fn looks_like_guid(name: &str) -> bool {
    let name = name.trim_matches(['{', '}']);
    name.len() == 36
        && name.split('-').map(str::len).eq([8, 4, 4, 4, 12])
        && name.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Служебный ли узел: тип из списка, плоскость по умолчанию (номера 2–4),
/// папка-закрывашка `…___EndTag___`, имя-идентификатор или пустое имя.
fn is_service(id: i64, name: &str, kind: &str) -> bool {
    name.is_empty()
        || SERVICE_KINDS.contains(&kind)
        || (2..=4).contains(&id)
        || name.ends_with("___EndTag___")
        || looks_like_guid(name)
}

impl Dimension {
    /// Размер из текста элемента `Dimension`.
    pub fn read(name: &str, shown: &str) -> Self {
        let shown = shown.trim();
        let (mark, digits) = if let Some(rest) = shown.strip_prefix("<MOD-DIAM>") {
            (Mark::Diameter, rest)
        } else if let Some(rest) = shown.strip_suffix('°') {
            (Mark::Angle, rest)
        } else if let Some(rest) = shown.strip_prefix('R') {
            (Mark::Radius, rest)
        } else {
            (Mark::Plain, shown)
        };
        let value = digits
            .trim()
            .replace(',', ".")
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite());
        Self {
            name: name.to_string(),
            shown: shown.to_string(),
            value,
            mark: if value.is_some() { mark } else { Mark::Text },
        }
    }
}

fn operation(node: &Node, tag: Tag) -> Option<Operation> {
    let id = node.number("id").filter(|id| *id >= 0)?;
    let name = node.attr("Name").unwrap_or("").to_string();
    let kind = node.attr("Type").unwrap_or("").to_string();
    let numbers = |text: &str| -> Vec<i64> {
        text.split(|c: char| !c.is_ascii_digit())
            .filter_map(|n| n.parse().ok())
            .collect()
    };
    Some(Operation {
        id,
        tag,
        service: is_service(id, &name, &kind),
        children: node
            .attr("DissectableChildren")
            .map(numbers)
            .unwrap_or_default(),
        dimensions: node
            .named("Dimension")
            .map(|d| Dimension::read(d.attr("Name").unwrap_or(""), &d.text))
            .collect(),
        name,
        kind,
    })
}

impl Tree {
    /// Разобрать корень `Keywords`. `None` — корень другой.
    pub fn read(root: &Node) -> Option<Self> {
        if !root.local().eq_ignore_ascii_case("Keywords") {
            return None;
        }
        let mut tree = Self {
            root_id: root.number("id"),
            root_name: root.attr("Name").unwrap_or("").to_string(),
            ..Self::default()
        };
        for node in &root.children {
            let tag = match node.local() {
                "Configuration" => {
                    tree.configurations.push(Configuration {
                        id: node.number("id"),
                        name: node.attr("Name").unwrap_or("").to_string(),
                        material: node
                            .attr("Material")
                            .filter(|m| !m.is_empty())
                            .map(str::to_string),
                    });
                    continue;
                }
                "Extrusion" => Tag::Extrusion,
                "Cut" => Tag::Cut,
                "Sketch" => Tag::Sketch,
                "Feature" => Tag::Feature,
                "Reference" => Tag::Reference,
                // Листы чертежа, слова приложений, заметки — не операции.
                _ => continue,
            };
            match operation(node, tag) {
                Some(found) => tree.operations.push(found),
                None => tree.skipped += 1,
            }
        }
        tree.operations.sort_by_key(|o| o.id);
        let before = tree.operations.len();
        tree.operations.dedup_by_key(|o| o.id);
        tree.skipped += before - tree.operations.len();
        Some(tree)
    }

    /// Операция по её постоянному номеру.
    pub fn operation(&self, id: i64) -> Option<&Operation> {
        self.operations
            .binary_search_by_key(&id, |o| o.id)
            .ok()
            .map(|at| &self.operations[at])
    }
}
