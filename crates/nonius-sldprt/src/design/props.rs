//! Свойства из секций `docProps/*.xml` (`docs/formats/sldprt-design.md`
//! §2.2, §2.3, §6).
//!
//! - `ISolidWorksInformation.xml` — свойства «по умолчанию». Берём только
//!   названные ниже: единицы, таблицу параметров, обозначение, сохранённую
//!   массу, модели видов чертежа. Остальное не читаем вовсе — там имя
//!   учётной записи автора (`SW-Last Saved By`) и другие личные данные,
//!   которым незачем попадать ни в ответы, ни в отладочный вывод.
//! - `Config-N-Properties.xml` — свойства конфигурации `N`, заполненные
//!   человеком: по шаблону ГОСТ это «Обозначение», «Наименование»,
//!   «Масса», «Материал»…
//! - `custom.xml` — свойства документа (вкладка «Настройка»).
//!
//! Сохранённые числа (масса) могли устареть: SolidWorks пишет то, что
//! показывал при сохранении.

use std::collections::BTreeMap;

use crate::xml::Node;

/// Свойство, заполненное человеком.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    /// Как показано: `1.21`.
    pub shown: String,
    /// Вычисленное значение с полной точностью, если SolidWorks его
    /// сохранил: `1.2131691352424556`.
    pub exact: Option<String>,
}

/// Сохранённые масс-свойства конфигурации. Обычно округлены до показа.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassProps {
    /// Центр масс, м.
    pub center_m: [f64; 3],
    /// Объём, м³.
    pub volume_m3: f64,
    /// Площадь, м².
    pub area_m2: f64,
    /// Масса, кг.
    pub mass_kg: f64,
}

/// Модель, показанная на виде чертежа.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawingModel {
    /// Полный путь модели при сохранении.
    pub path: String,
    /// Конфигурация модели.
    pub configuration: String,
    /// Имя вида.
    pub view: String,
    /// Имя листа.
    pub sheet: String,
}

/// Свойства документа.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Props {
    /// Единицы длины документа: `0` — миллиметры.
    pub units_linear: Option<i64>,
    /// Знаков после запятой при показе длин.
    pub decimals: Option<i64>,
    /// Размеры ведёт таблица параметров (Excel).
    pub design_table: Option<bool>,
    /// Обозначение в спецификации.
    pub bom_number: Option<String>,
    /// Сохранённые масс-свойства по номеру конфигурации.
    pub mass: BTreeMap<i64, MassProps>,
    /// Свойства документа, заполненные человеком.
    pub document: BTreeMap<String, Property>,
    /// Свойства конфигураций по номеру конфигурации.
    pub configurations: BTreeMap<i64, BTreeMap<String, Property>>,
    /// Модели видов чертежа.
    pub drawing_models: Vec<DrawingModel>,
}

/// Значение свойства: текст первого потомка `vt:*`.
fn value(property: &Node) -> Option<&str> {
    property
        .children
        .iter()
        .find(|c| c.name.starts_with("vt:"))
        .map(|c| c.text.as_str())
}

/// Все свойства секции с именем `section` (или всех секций, если `None`).
fn properties<'a>(root: &'a Node, section: Option<&'a str>) -> impl Iterator<Item = &'a Node> {
    root.named("propertySection")
        .filter(move |s| section.is_none_or(|name| s.attr("name") == Some(name)))
        .flat_map(|s| s.named("property"))
}

/// Свойства, заполненные человеком: с именем и непустым значением.
fn user_defined(root: &Node) -> BTreeMap<String, Property> {
    properties(root, Some("UserDefinedProperties"))
        .filter_map(|p| {
            let name = p.attr("name").filter(|n| !n.is_empty())?;
            let shown = value(p).filter(|v| !v.is_empty())?;
            // Точное значение записано как `<$5$1.2131691352424556$>`.
            let exact = p
                .child("FPVals")
                .and_then(value)
                .and_then(|v| v.strip_prefix("<$")?.strip_suffix("$>"))
                .and_then(|v| v.split_once('$'))
                .map(|(_, exact)| exact.to_string())
                .filter(|exact| exact != shown);
            let property = Property {
                shown: shown.to_string(),
                exact,
            };
            Some((name.to_string(), property))
        })
        .collect()
}

/// `SW-MassProp-Config-N`: 14 чисел через запятую.
fn mass_props(text: &str) -> Option<MassProps> {
    let n: Vec<f64> = text
        .split(',')
        .map(|n| n.trim().parse::<f64>().ok().filter(|v| v.is_finite()))
        .collect::<Option<_>>()?;
    (n.len() >= 6).then(|| MassProps {
        center_m: [n[0], n[1], n[2]],
        volume_m3: n[3],
        area_m2: n[4],
        mass_kg: n[5],
    })
}

/// `SW-Config-Model-View-Sheet-List`: `<число видов>:@@`, дальше по пять
/// ячеек на вид через `@`: `<конфигурация>@0@<полный путь>@<вид>@<лист>`.
fn drawing_models(text: &str) -> Vec<DrawingModel> {
    let Some((_, groups)) = text.split_once(":@") else {
        return Vec::new();
    };
    let cells: Vec<&str> = groups.split('@').collect();
    // Перед первой группой — пустая ячейка от её `@`.
    let cells = cells.strip_prefix(&[""]).unwrap_or(&cells);
    cells
        .chunks_exact(5)
        .map(|group| DrawingModel {
            configuration: group[0].to_string(),
            path: group[2].to_string(),
            view: group[3].to_string(),
            sheet: group[4].to_string(),
        })
        .collect()
}

impl Props {
    /// Добавить свойства «по умолчанию» (`ISolidWorksInformation.xml`).
    /// Читаются только поля из списка — см. начало файла.
    pub fn read_information(&mut self, root: &Node) {
        for property in properties(root, None) {
            let (Some(name), Some(text)) = (property.attr("name"), value(property)) else {
                continue;
            };
            let number = || text.trim().parse::<i64>().ok();
            match name {
                "SW_UnitsLinear" => self.units_linear = number(),
                "SW_UnitsLinearDecimalPlaces" => self.decimals = number(),
                "SW-HasDesignTable" => self.design_table = number().map(|n| n != 0),
                "SW-BOM Part Number" => {
                    self.bom_number = Some(text.to_string()).filter(|t| !t.is_empty());
                }
                "SW-Config-Model-View-Sheet-List" => self.drawing_models = drawing_models(text),
                _ => {
                    let config = name
                        .strip_prefix("SW-MassProp-Config-")
                        .and_then(|n| n.parse::<i64>().ok());
                    if let (Some(config), Some(mass)) = (config, mass_props(text)) {
                        self.mass.insert(config, mass);
                    }
                }
            }
        }
    }

    /// Добавить свойства документа (`custom.xml`).
    pub fn read_document(&mut self, root: &Node) {
        self.document = user_defined(root);
    }

    /// Добавить свойства конфигурации `config` (`Config-N-Properties.xml`).
    pub fn read_configuration(&mut self, config: i64, root: &Node) {
        self.configurations.insert(config, user_defined(root));
    }

    /// Свойство конфигурации `config`, а если там нет — документа.
    pub fn property(&self, config: i64, name: &str) -> Option<&Property> {
        self.configurations
            .get(&config)
            .and_then(|c| c.get(name))
            .or_else(|| self.document.get(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_property_lists_models_of_views() {
        let one =
            drawing_models("1:@@По умолчанию<Как обработанный>@0@C:\\п\\вал.SLDPRT@Вид1@Лист1");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].path, "C:\\п\\вал.SLDPRT");
        assert_eq!(one[0].configuration, "По умолчанию<Как обработанный>");
        assert_eq!(
            (one[0].view.as_str(), one[0].sheet.as_str()),
            ("Вид1", "Лист1")
        );
        let three = drawing_models(
            "3:@@К@0@C:\\a.SLDPRT@В1@Л1@К@0@C:\\a.SLDPRT@В2@Л1@К@0@C:\\b.SLDASM@В3@Л2",
        );
        let paths: Vec<&str> = three.iter().map(|m| m.path.as_str()).collect();
        assert_eq!(paths, ["C:\\a.SLDPRT", "C:\\a.SLDPRT", "C:\\b.SLDASM"]);
        assert_eq!(three[2].sheet, "Л2");
        assert!(drawing_models("мусор").is_empty());
        assert!(drawing_models("2:@@К@0").is_empty());
    }

    #[test]
    fn mass_is_read_as_plain_numbers() {
        let mass = mass_props("0.08, 0.05, -0.01, 0.000436, 0.1628, 1.2131691352424556, 0.1, 0.2, 0.3, 0, 0, 0, 1.000000, 0.000000");
        assert_eq!(mass.unwrap().mass_kg, 1.2131691352424556);
        assert_eq!(mass_props("1, 2, x"), None);
        assert_eq!(mass_props("1, 2"), None);
    }
}
