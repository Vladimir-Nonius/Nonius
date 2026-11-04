//! Откуда окно берёт детали, версии и сравнения.
//!
//! Окно не знает про git и OpenCascade: всё приходит через [`ProjectSource`].
//! Реализацию даёт программа: `nonius-cli` поверх `nonius-app`.

mod entries;

pub use entries::*;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nonius_scene::Scene;

/// Системные окна выбора. Окно спрашивает через этот трейт: на нём держатся
/// проверки окна, где человека нет.
pub trait Chooser: Send + Sync {
    /// Выбрать папку. `typed` — путь из поля ввода, если окна выбора нет.
    fn folder(&self, typed: &str) -> Option<PathBuf>;
    /// Выбрать файлы моделей.
    fn models(&self) -> Vec<PathBuf>;
    /// Выбрать файл STEP.
    fn step(&self) -> Option<PathBuf>;
    /// Выбрать любые файлы: технология, чертёж, ТЗ.
    fn files(&self) -> Vec<PathBuf>;
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
    /// Папки-группы, путь от корня через `/`, родитель раньше детей.
    fn folders(&self) -> anyhow::Result<Vec<String>>;
    /// Детали и папки вместе — для дерева.
    fn listing(&self) -> anyhow::Result<Listing> {
        Ok(Listing {
            parts: self.parts()?,
            folders: self.folders()?,
        })
    }
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
    /// Создать пустую деталь или сборку в папке (пустая строка — корень).
    fn new_part(&self, name: &str, assembly: bool, folder: &str) -> anyhow::Result<()>;
    /// Создать папку-группу: путь от корня через `/`.
    fn new_folder(&self, folder: &str) -> anyhow::Result<()>;
    /// Перенести деталь в папку (пустая строка — корень): версии остаются
    /// при ней.
    fn move_part(&self, part: &str, folder: &str) -> anyhow::Result<()>;
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
    /// Положить копию своего файла в `docs/` детали. Возвращает путь внутри
    /// детали; в историю файл попадёт со следующей версией.
    fn add_file(&self, part: &str, file: &Path) -> anyhow::Result<String>;
    /// Убрать файл из папки детали: в сохранённых версиях он остаётся.
    fn remove_file(&self, part: &str, path: &str) -> anyhow::Result<()>;
    /// Убрать версию с карты (`hidden`) или вернуть на неё.
    fn set_hidden(&self, part: &str, version: u32, hidden: bool) -> anyhow::Result<()>;
    /// Выгрузить файлы версии в папку. Возвращает, сколько файлов записано.
    fn export_version(&self, part: &str, version: u32, dest: &Path) -> anyhow::Result<usize>;
    /// Проект старого формата (`parts/`, `assemblies/`): только просмотр,
    /// пока его не обновят.
    fn old_layout(&self) -> bool {
        false
    }
    /// Перевести проект старого формата на папки. После этого его надо
    /// открыть заново.
    fn upgrade(&self) -> anyhow::Result<()> {
        Ok(())
    }
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

/// Переносит закрытый проект из первой папки во вторую: «Сохранить проект как…».
pub type Mover = Arc<dyn Fn(&Path, &Path) -> anyhow::Result<()> + Send + Sync>;

/// Убирает папку несохранённого черновика с деталями: в программе — в
/// Корзину, откуда её можно вернуть.
pub type Discard = Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync>;

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
            folder: String::new(),
        }
    }

    fn v(n: u32) -> VersionEntry {
        VersionEntry {
            number: n,
            date: String::new(),
            author: String::new(),
            summary: String::new(),
            hidden: false,
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
