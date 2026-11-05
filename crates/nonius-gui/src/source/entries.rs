//! Что окно получает от проекта: детали, версии, файлы, массы.

use std::path::PathBuf;

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
    /// Файл модели, который открывает правый щелчок по детали.
    pub model: Option<String>,
    /// Отпечаток модели и паспорта: изменился — надо показать заново.
    pub stamp: String,
    /// Папка, в которой лежит деталь: путь от корня через `/`, пустой — корень.
    pub folder: String,
}

/// Настройки проекта, которые меняют в окне «Настройки».
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSettings {
    /// Название проекта.
    pub name: String,
    /// Допуск сравнения, мм.
    pub tolerance_mm: f64,
}

/// Всё, что рисует дерево: детали и папки-группы.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    /// Детали и сборки.
    pub parts: Vec<PartEntry>,
    /// Папки-группы, путь от корня через `/`, родитель раньше детей.
    pub folders: Vec<String>,
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
    /// Убрана с карты: видна только в списке «Убрано с карты».
    pub hidden: bool,
}

/// Какую версию показать.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Pick {
    /// Сохранённая версия.
    Version(u32),
    /// То, что сейчас лежит в папке детали.
    #[default]
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
    /// Файл SolidWorks менялся, а STEP нет: форма на экране старая.
    pub step_behind: bool,
}

/// Что получилось при добавлении модели.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedModel {
    /// Деталь.
    pub part: String,
    /// Новая версия; `None` — файл такой же, как последняя версия.
    pub version: Option<u32>,
    /// Что не получилось, хотя модель добавлена.
    pub warnings: Vec<String>,
}

/// Удалённая деталь, которую можно вернуть.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedEntry {
    /// Имя.
    pub name: String,
    /// Сборка, а не деталь.
    pub assembly: bool,
    /// Когда удалена, для показа.
    pub date: String,
    /// Кто удалил.
    pub author: String,
}

/// С чем сравнить деталь, не сохраняя.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Other {
    /// Файл STEP на диске.
    File(PathBuf),
    /// Другая деталь, как она есть сейчас.
    Part(String),
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

/// Что случилось с файлом между двумя версиями.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    /// Не менялся.
    Same,
    /// Новый.
    Added,
    /// Изменён.
    Modified,
    /// Удалён.
    Deleted,
}

/// Файл детали на вкладке «Файлы».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Путь внутри папки детали.
    pub path: String,
    /// Что с ним случилось.
    pub state: FileState,
    /// Размер в «Было», байт; `None` — файла там нет.
    pub before: Option<u64>,
    /// Размер в «Стало», байт; `None` — файла там нет.
    pub after: Option<u64>,
    /// Текстовый: можно показать разницу строк.
    pub text: bool,
}

/// Программа, которая умеет открывать файл.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramEntry {
    /// Название для показа.
    pub name: String,
    /// Строка запуска из настроек системы.
    pub command: String,
}

/// Чем открыть файл.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenWay {
    /// Программой, назначенной в системе.
    Default,
    /// Этой программой.
    Program(ProgramEntry),
    /// Спросить в окне системы «Открыть с помощью».
    Choose,
    /// Не открывать, а показать в папке.
    Folder,
}

/// Вид строки в разнице текста.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// Без изменений.
    Same,
    /// Добавлена.
    Added,
    /// Убрана.
    Removed,
    /// Здесь пропущены строки без изменений.
    Gap,
}

/// Строка в разнице текста.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEntry {
    /// Вид строки.
    pub kind: DiffKind,
    /// Текст.
    pub text: String,
}
