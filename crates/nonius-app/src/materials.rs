//! Справочник частых материалов с плотностью.
//!
//! Плотности — справочные, кг/м³, при комнатной температуре. Если нужного
//! материала нет, механик вводит плотность сам. Плотность «по умолчанию»
//! программа не подставляет никогда.

/// Материал из справочника.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    /// Название, как его пишут на чертеже.
    pub name: &'static str,
    /// Другие написания, по которым материал тоже находится.
    pub aliases: &'static [&'static str],
    /// Плотность, кг/м³.
    pub density_kg_m3: f64,
}

const fn m(name: &'static str, aliases: &'static [&'static str], density: f64) -> Material {
    Material {
        name,
        aliases,
        density_kg_m3: density,
    }
}

/// Справочник. Порядок — как в списке выбора.
pub const MATERIALS: &[Material] = &[
    m("Ст3", &["Ст3сп", "Ст3пс", "Сталь Ст3", "S235"], 7850.0),
    m("Сталь 20", &["Ст20"], 7850.0),
    m("Сталь 45", &["Ст45", "C45"], 7850.0),
    m("09Г2С", &["Сталь 09Г2С", "S355"], 7850.0),
    m("40Х", &["Сталь 40Х"], 7820.0),
    m("65Г", &["Сталь 65Г"], 7850.0),
    m("12Х18Н10Т", &["Сталь 12Х18Н10Т", "AISI 321"], 7900.0),
    m("08Х18Н10", &["AISI 304"], 7900.0),
    m("AISI 316", &["10Х17Н13М2Т"], 8000.0),
    m("СЧ20", &["Чугун СЧ20"], 7100.0),
    m("АМг3", &["5754"], 2670.0),
    m("АМг6", &[], 2640.0),
    m("Д16Т", &["Д16", "2024"], 2780.0),
    m("АД31", &["6063"], 2710.0),
    m("6061", &["АД33", "6061-T6"], 2700.0),
    m("Латунь Л63", &["Л63"], 8440.0),
    m("Бронза БрАЖ9-4", &["БрАЖ9-4"], 7600.0),
    m("Медь М1", &["М1"], 8940.0),
    m("Титан ВТ1-0", &["ВТ1-0"], 4505.0),
    m("Капролон", &["ПА6", "Полиамид 6"], 1150.0),
    m("Полиацеталь", &["POM", "ПОМ"], 1410.0),
    m("Фторопласт-4", &["PTFE", "Ф4"], 2200.0),
    m("Оргстекло", &["ПММА", "PMMA"], 1190.0),
    m("ABS", &["АБС"], 1050.0),
    m("PLA", &[], 1240.0),
    m("PETG", &[], 1270.0),
];

/// Найти материал по названию: без учёта регистра, пробелов и дефисов.
pub fn find_material(name: &str) -> Option<&'static Material> {
    let key = normalize(name);
    if key.is_empty() {
        return None;
    }
    MATERIALS
        .iter()
        .find(|m| normalize(m.name) == key || m.aliases.iter().any(|a| normalize(a) == key))
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(char::to_lowercase)
        .map(|c| if c == 'ё' { 'е' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_by_name_and_alias() {
        assert_eq!(find_material("ст3").unwrap().density_kg_m3, 7850.0);
        assert_eq!(find_material("Сталь 09Г2С").unwrap().name, "09Г2С");
        assert_eq!(find_material("aisi304").unwrap().name, "08Х18Н10");
        assert_eq!(find_material(" АМг 6 ").unwrap().density_kg_m3, 2640.0);
        assert!(find_material("унобтаниум").is_none());
        assert!(find_material("  ").is_none());
    }

    #[test]
    fn table_is_sane() {
        let mut seen = std::collections::BTreeSet::new();
        for m in MATERIALS {
            assert!(
                m.density_kg_m3 > 500.0 && m.density_kg_m3 < 23000.0,
                "{m:?}"
            );
            for n in std::iter::once(m.name).chain(m.aliases.iter().copied()) {
                assert!(seen.insert(normalize(n)), "повтор названия {n}");
            }
        }
    }
}
