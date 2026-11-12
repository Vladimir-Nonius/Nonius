//! Разбор папки для окна: план (что станет деталями и версиями), вопросы
//! человеку и итог раскладки. Считает всё программа; окно только показывает.

use std::path::PathBuf;

/// План разбора папки.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IntakePlan {
    /// Разбираемая папка.
    pub source: PathBuf,
    /// Будущие детали и сборки, по папкам проекта.
    pub items: Vec<IntakeItem>,
    /// Вопросы там, где программа не уверена. План выполним и без ответов.
    pub questions: Vec<IntakeQuestion>,
    /// Чертежи и бумаги, которые останутся на месте: имя файла и почему.
    pub left: Vec<(String, String)>,
    /// Сколько ещё файлов не тронуто: не модели и не чертежи.
    pub other: usize,
}

/// Будущая деталь или сборка.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IntakeItem {
    /// Имя в проекте.
    pub name: String,
    /// Сборка, а не деталь.
    pub assembly: bool,
    /// Папка проекта через `/`, пустая — корень.
    pub folder: String,
    /// Сколько версий будет сохранено.
    pub versions: usize,
    /// Почему так решено и что изменилось между версиями — готовые строки.
    pub why: Vec<String>,
    /// Чертежи и бумаги, которые лягут к детали: имена файлов.
    pub docs: Vec<String>,
    /// Догадка, которую форма деталей не подтвердила: стоит проверить.
    pub unsure: bool,
}

/// Вопрос человеку.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IntakeQuestion {
    /// По нему передаётся ответ.
    pub id: String,
    /// Сам вопрос.
    pub text: String,
    /// Улики: что программа увидела в файлах.
    pub facts: Vec<String>,
    /// Что думает программа; `None` — мнения нет.
    pub suggested: Option<bool>,
}

/// Что получилось после раскладки.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IntakeOutcome {
    /// Сколько деталей и сборок разложено.
    pub parts: usize,
    /// Сколько версий сохранено.
    pub versions: u32,
    /// Всё сделано, но что-то не так.
    pub warnings: Vec<String>,
    /// Что не вышло: имя детали и почему.
    pub failed: Vec<(String, String)>,
    /// Человек остановил: остальное не тронуто.
    pub stopped: bool,
}
