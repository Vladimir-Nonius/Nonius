//! Откуда окно берёт детали, версии и сравнения.
//!
//! Окно не знает про git и OpenCascade: всё приходит через [`ProjectSource`].
//! Реализацию даёт программа: `nonius-cli` поверх `nonius-app`.

use std::path::Path;
use std::sync::Arc;

use nonius_core::compare::ChangeReport;
use nonius_scene::Scene;

/// Деталь в списке.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartEntry {
    /// Имя.
    pub name: String,
    /// Сборка, а не деталь.
    pub assembly: bool,
    /// Последняя сохранённая версия: `v3`.
    pub latest: Option<String>,
    /// Есть что сохранить: файлы изменены после последней версии или у новой
    /// детали ещё нет ни одной версии.
    pub dirty: bool,
}

/// Версия детали.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionEntry {
    /// Номер.
    pub number: u32,
    /// Дата для показа.
    pub date: String,
    /// Автор.
    pub author: String,
    /// Первая строка описания.
    pub summary: String,
}

/// Какую версию показать.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pick {
    /// Сохранённая версия.
    Version(u32),
    /// То, что сейчас лежит в папке детали.
    Current,
}

/// Результат сравнения двух версий.
#[derive(Debug, Clone, Default)]
pub struct Comparison {
    /// Сцена для 3D; `None` — показать нечего (нет STEP или нет геометрии).
    pub scene: Option<Scene>,
    /// Почему сцены нет, если причина известна: «в STEP нет геометрии».
    pub scene_problem: Option<String>,
    /// Отчёты модулей сравнения: числа, материал, форма.
    pub reports: Vec<ChangeReport>,
}

/// Материал и масса детали в версии или сейчас.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MassEntry {
    /// Материал; `None` — не задан.
    pub material: Option<String>,
    /// Плотность, кг/м³.
    pub density_kg_m3: Option<f64>,
    /// Масса, кг.
    pub mass_kg: Option<f64>,
    /// Почему массы нет, словами: «материал не задан».
    pub no_mass: Option<String>,
}

/// Материал из справочника.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialEntry {
    /// Название.
    pub name: String,
    /// Плотность, кг/м³.
    pub density_kg_m3: f64,
}

/// Ход долгой работы и её отмена.
pub trait Progress: Sync {
    /// Начался шаг: «Сравниваю…».
    fn step(&self, what: &str);
    /// Человек нажал «Отменить». Работа проверяет это между шагами.
    fn cancelled(&self) -> bool {
        false
    }
}

/// Проект для окна. Вызовы блокирующие: окно делает их в фоне.
pub trait ProjectSource: Send + Sync {
    /// Название проекта.
    fn title(&self) -> String;
    /// Папка проекта.
    fn root(&self) -> &Path;
    /// Детали и сборки.
    fn parts(&self) -> anyhow::Result<Vec<PartEntry>>;
    /// Версии детали, новые первыми.
    fn versions(&self, part: &str) -> anyhow::Result<Vec<VersionEntry>>;
    /// Сравнить две версии. `before` — всегда сохранённая версия;
    /// `before == after` — показать одну версию без сравнения.
    /// `progress` получает короткие сообщения о ходе работы и говорит,
    /// не отменено ли сравнение.
    fn compare(
        &self,
        part: &str,
        before: Pick,
        after: Pick,
        progress: &dyn Progress,
    ) -> anyhow::Result<Comparison>;
    /// Изменённые с последней версии файлы детали, для показа: «~ кронштейн.step».
    fn changed_files(&self, part: &str) -> anyhow::Result<Vec<String>>;
    /// Сохранить текущее состояние детали новой версией. Возвращает её номер.
    fn save_version(&self, part: &str, message: &str) -> anyhow::Result<u32>;
    /// Материал и масса детали в версии или сейчас.
    fn mass(&self, part: &str, pick: Pick) -> anyhow::Result<MassEntry>;
    /// Справочник материалов, в порядке списка выбора.
    fn materials(&self) -> Vec<MaterialEntry>;
    /// Назначить материал детали сейчас: название и плотность. Плотность
    /// `None` — взять из справочника. Название `None` — убрать материал.
    fn set_material(
        &self,
        part: &str,
        material: Option<&str>,
        density_kg_m3: Option<f64>,
    ) -> anyhow::Result<MassEntry>;
}

/// Открывает проект по папке (для «Открыть папку проекта…» и перетаскивания).
pub type Opener = Arc<dyn Fn(&Path) -> anyhow::Result<Box<dyn ProjectSource>> + Send + Sync>;

/// Что сравнить по умолчанию: изменённое — с последней версией; иначе —
/// последнюю с предпоследней; одна версия — её саму. Версий нет — сравнивать
/// не с чем, окно просит сохранить первую.
pub fn default_picks(part: &PartEntry, versions: &[VersionEntry]) -> (Pick, Pick) {
    match versions {
        [] => (Pick::Current, Pick::Current),
        [last, ..] if part.dirty => (Pick::Version(last.number), Pick::Current),
        [only] => (Pick::Version(only.number), Pick::Version(only.number)),
        [last, prev, ..] => (Pick::Version(prev.number), Pick::Version(last.number)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(dirty: bool) -> PartEntry {
        PartEntry {
            name: "кронштейн".into(),
            assembly: false,
            latest: None,
            dirty,
        }
    }

    fn v(n: u32) -> VersionEntry {
        VersionEntry {
            number: n,
            date: String::new(),
            author: String::new(),
            summary: String::new(),
        }
    }

    #[test]
    fn picks() {
        use Pick::*;
        assert_eq!(default_picks(&part(false), &[]), (Current, Current));
        assert_eq!(default_picks(&part(true), &[]), (Current, Current));
        assert_eq!(
            default_picks(&part(false), &[v(1)]),
            (Version(1), Version(1))
        );
        assert_eq!(default_picks(&part(true), &[v(1)]), (Version(1), Current));
        assert_eq!(
            default_picks(&part(false), &[v(3), v(2), v(1)]),
            (Version(2), Version(3))
        );
        assert_eq!(
            default_picks(&part(true), &[v(3), v(2), v(1)]),
            (Version(3), Current)
        );
    }
}
