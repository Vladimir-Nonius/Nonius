//! Откуда окно берёт детали, версии и сравнения.
//!
//! Окно не знает про git и OpenCascade: всё приходит через [`ProjectSource`].
//! Реализацию даёт программа: `nonius-cli` поверх `nonius-app`.

use std::path::{Path, PathBuf};
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
    /// Файл модели, который открывает правый щелчок по детали.
    pub model: Option<String>,
    /// Отпечаток модели и паспорта: изменился — надо показать заново.
    pub stamp: String,
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

/// Системные окна выбора. Окно спрашивает через этот трейт: на нём держатся
/// проверки окна, где человека нет.
pub trait Chooser: Send + Sync {
    /// Выбрать папку. `typed` — путь из поля ввода, если окна выбора нет.
    fn folder(&self, typed: &str) -> Option<PathBuf>;
    /// Выбрать файлы моделей.
    fn models(&self) -> Vec<PathBuf>;
    /// Выбрать файл STEP.
    fn step(&self) -> Option<PathBuf>;
    /// Куда сохранить картинку с предложенным именем.
    fn picture(&self, name: &str) -> Option<PathBuf>;
}

/// Что есть на компьютере для истории версий.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Readiness {
    /// Установлен git.
    pub git: bool,
    /// Установлен Git LFS.
    pub lfs: bool,
    /// Имя автора версий задано.
    pub author: bool,
}

/// Компьютер, на котором работает окно. Спрашивается при запуске.
pub trait Machine: Send + Sync {
    /// Что здесь уже есть.
    fn readiness(&self) -> Readiness;
    /// Запомнить имя автора версий.
    fn set_author(&self, name: &str) -> anyhow::Result<()>;
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

/// Ход долгой работы и её отмена.
pub trait Progress: Sync {
    /// Начался шаг: «Сравниваю…».
    fn step(&self, what: &str);
    /// Деталь готова, цвета ещё считаются: её можно показать.
    fn model(&self, scene: Scene) {
        let _ = scene;
    }
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
    /// Сравнить две версии. `before` — сохранённая версия; `before == after`
    /// — показать одну версию или текущее состояние без сравнения.
    /// `progress` получает короткие сообщения о ходе работы и саму деталь,
    /// пока считаются цвета, и говорит, не отменено ли сравнение.
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
    /// Файлы «Стало» и что изменилось после «Было». `before == after` —
    /// только список файлов, без изменений.
    fn files(&self, part: &str, before: Pick, after: Pick) -> anyhow::Result<Vec<FileEntry>>;
    /// Строки текстового файла. `before == after` — весь файл.
    fn file_diff(
        &self,
        part: &str,
        before: Pick,
        after: Pick,
        path: &str,
    ) -> anyhow::Result<Vec<DiffEntry>>;
    /// Открыть файл или показать в папке. Файл версии открывается копией.
    /// Возвращает, что именно открыто.
    fn open_file(
        &self,
        part: &str,
        pick: Pick,
        path: &str,
        way: &OpenWay,
    ) -> anyhow::Result<PathBuf>;
    /// Программы, которые система знает для такого файла.
    fn programs(&self, path: &str) -> Vec<ProgramEntry>;
    /// Создать пустую деталь или сборку.
    fn new_part(&self, name: &str, assembly: bool) -> anyhow::Result<()>;
    /// Положить файл модели в деталь `part` (её может ещё не быть) и сразу
    /// сохранить новую версию с описанием `message`. Файл, который не
    /// читается, ничего не меняет.
    fn add_model(&self, file: &Path, part: &str, message: &str) -> anyhow::Result<AddedModel>;
    /// Переименовать деталь: версии остаются при ней.
    fn rename_part(&self, part: &str, new: &str) -> anyhow::Result<()>;
    /// Удалить деталь: она остаётся в истории, несохранённое становится версией.
    fn delete_part(&self, part: &str) -> anyhow::Result<()>;
    /// Удалённые детали, новые первыми.
    fn deleted_parts(&self) -> anyhow::Result<Vec<DeletedEntry>>;
    /// Вернуть удалённую деталь.
    fn recover_part(&self, part: &str) -> anyhow::Result<()>;
    /// Вернуть деталь к версии: появится новая версия. Возвращает её номер.
    fn restore_version(&self, part: &str, version: u32) -> anyhow::Result<u32>;
    /// Выгрузить файлы версии в папку. Возвращает, сколько файлов записано.
    fn export_version(&self, part: &str, version: u32, dest: &Path) -> anyhow::Result<usize>;
    /// Сравнить деталь с файлом или другой деталью, ничего не сохраняя.
    fn compare_outside(
        &self,
        part: &str,
        pick: Pick,
        other: &Other,
        progress: &dyn Progress,
    ) -> anyhow::Result<Comparison>;
}

/// Открывает или создаёт проект по папке: «Открыть проект…», «Создать
/// проект…», перетаскивание.
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
            model: None,
            stamp: String::new(),
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
