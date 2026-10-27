//! Откуда окно берёт детали, версии и сравнения.
//!
//! Окно не знает про git и OpenCascade: всё приходит через [`ProjectSource`].
//! Реализацию даёт программа (сейчас — `nonius-cli`, потом — `nonius-app`).

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
    /// Файлы изменены после последней версии.
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
    /// Сравнить две версии. `before == after` — показать одну версию без сравнения.
    /// `progress` получает короткие сообщения о ходе работы.
    fn compare(
        &self,
        part: &str,
        before: Pick,
        after: Pick,
        progress: &dyn Fn(&str),
    ) -> anyhow::Result<Comparison>;
}

/// Открывает проект по папке (для «Открыть папку проекта…» и перетаскивания).
pub type Opener = Arc<dyn Fn(&Path) -> anyhow::Result<Box<dyn ProjectSource>> + Send + Sync>;

/// Что сравнить по умолчанию: изменённое — с последней версией; иначе —
/// последнюю с предпоследней; одна версия — её саму; версий нет — текущее.
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
