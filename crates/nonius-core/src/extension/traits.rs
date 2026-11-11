//! Трейты точек расширения и данные, которыми ядро обменивается с модулями.

use std::path::{Path, PathBuf};

use super::Module;
use crate::compare::{ChangeReport, CompareContext};
use crate::error::Result;
use crate::meta::PartMeta;
use crate::names::{PartName, VersionNumber};
use crate::tree::FeatureTree;

/// Версия детали, как её видят модули: папка с файлами и разобранные данные.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// Деталь или сборка.
    pub item: PartName,
    /// Номер версии, если это зафиксированная версия.
    pub version: Option<VersionNumber>,
    /// Папка с файлами версии (`model.step`, `docs/`, …).
    pub dir: PathBuf,
    /// Паспорт версии.
    pub meta: PartMeta,
    /// Дерево построения, если есть.
    pub tree: Option<FeatureTree>,
}

/// Источник геометрии: превращает файл модели в паспорт версии.
pub trait GeometrySource: Module {
    /// Умеет ли модуль читать этот файл (обычно по расширению).
    fn can_read(&self, path: &Path) -> bool;
    /// Прочитать файл: контрольные числа, материал, свойства.
    fn read(&self, path: &Path) -> Result<PartMeta>;
}

/// Источник дерева построения.
pub trait TreeSource: Module {
    /// Умеет ли модуль читать этот файл.
    fn can_read(&self, path: &Path) -> bool;
    /// Прочитать дерево построения.
    fn read_tree(&self, path: &Path) -> Result<FeatureTree>;
}

/// Сравнение двух версий.
pub trait Comparator: Module {
    /// Различия от `before` к `after`. Одинаковые версии дают пустой отчёт.
    fn compare(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        ctx: &CompareContext,
    ) -> Result<ChangeReport>;

    /// Быстрое ли это сравнение: отвечает за доли секунды и годится там,
    /// где сравнивают много пар подряд (разбор папки показывает, что
    /// изменилось между найденными версиями). Сравнение, которое считает
    /// разницу объёмов, быстрым не называется.
    fn quick(&self) -> bool {
        false
    }
}

/// Отрисовка изменений: картинки, GIF, 3D-сцены.
pub trait Renderer: Module {
    /// Нарисовать изменения в `out_dir`. Возвращает созданные файлы.
    fn render(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        report: &ChangeReport,
        out_dir: &Path,
    ) -> Result<Vec<PathBuf>>;

    /// То же с параметрами сравнения (допуск проекта). Программа вызывает
    /// именно этот метод. Модулю, чей рисунок зависит от допуска, стоит
    /// его переопределить: тогда рисунок совпадёт с отчётом сравнения.
    /// По умолчанию параметры не нужны — вызывается [`Renderer::render`].
    fn render_with(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        report: &ChangeReport,
        ctx: &CompareContext,
        out_dir: &Path,
    ) -> Result<Vec<PathBuf>> {
        let _ = ctx;
        self.render(before, after, report, out_dir)
    }
}

/// Экспорт файлов из версии.
pub trait Exporter: Module {
    /// Форматы, которые модуль умеет выдавать: `dxf`, `pdf`, `stl`.
    fn formats(&self) -> Vec<String>;
    /// Выгрузить версию в формате `format` в папку `out_dir`.
    fn export(&self, snapshot: &Snapshot, format: &str, out_dir: &Path) -> Result<PathBuf>;
}

/// Перевод файла CAD в STEP: родной файл (например `.sldprt`) Nonius хранит
/// как есть, а форму, массу и сравнение считает по STEP. Модуль этой точки
/// получает STEP сам — обычно просит об этом установленную CAD-систему, —
/// чтобы человеку не выгружать его руками при каждом импорте.
pub trait Converter: Module {
    /// Умеет ли модуль перевести этот файл (обычно по расширению).
    fn can_convert(&self, path: &Path) -> bool;
    /// Записать STEP файла `source` в `target`. Сам `source` не меняется.
    fn to_step(&self, source: &Path, target: &Path) -> Result<()>;
}

/// Связи файлов CAD: из каких деталей собрана сборка, к какой модели
/// относится чертёж. По ним Nonius сам раскладывает папку с файлами: чертёж —
/// к своей детали, детали — к своей сборке. Знает это CAD-система, поэтому
/// модуль обычно спрашивает её (или аддин сообщает сам).
pub trait LinkSource: Module {
    /// Знает ли модуль связи таких файлов (обычно по расширению).
    fn knows(&self, path: &Path) -> bool;
    /// Для каждого файла из `files` — файлы, на которые он ссылается напрямую
    /// (полные пути). Ответ той же длины и в том же порядке; у файла без
    /// ссылок список пуст.
    fn links(&self, files: &[PathBuf]) -> Result<Vec<Vec<PathBuf>>>;
}

/// Событие проекта.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Event {
    /// Участник зафиксировал версию у себя.
    VersionCommitted {
        /// Деталь.
        item: PartName,
        /// Версия.
        version: VersionNumber,
    },
    /// Версия отправлена на утверждение.
    VersionProposed {
        /// Деталь.
        item: PartName,
        /// Версия.
        version: VersionNumber,
    },
    /// Лидер утвердил версию.
    VersionApproved {
        /// Деталь.
        item: PartName,
        /// Версия.
        version: VersionNumber,
        /// Кто утвердил.
        by: String,
    },
    /// Лидер отклонил версию.
    VersionRejected {
        /// Деталь.
        item: PartName,
        /// Версия.
        version: VersionNumber,
        /// Кто отклонил.
        by: String,
        /// Причина.
        reason: String,
    },
}

/// Обработчик событий: уведомления, проверки перед утверждением.
pub trait EventHandler: Module {
    /// Отреагировать на событие. Ошибка обработчика не отменяет событие.
    fn on_event(&self, event: &Event) -> Result<()>;
}
