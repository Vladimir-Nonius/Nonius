//! Трейты точек расширения и данные, которыми ядро обменивается с модулями.

use std::path::{Path, PathBuf};

use super::Module;
use crate::compare::{ChangeReport, CompareContext};
use crate::design::{Design, Partial};
use crate::error::Result;
use crate::meta::{GeometrySummary, PartMeta};
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
    ///
    /// Ошибка [`Error::Refused`](crate::Error::Refused) — отказ
    /// окончательный: пока файл и условия ([`stamp`](Self::stamp)) те же,
    /// повторять бессмысленно, и программа запомнит его. Любая другая
    /// ошибка — «сейчас не вышло»: в следующий раз попробуют снова.
    fn to_step(&self, source: &Path, target: &Path) -> Result<()>;

    /// Отпечаток условий перевода: всё, от чего зависит STEP файла `source`,
    /// кроме его собственного содержимого, — версия CAD-системы, её
    /// настройки выгрузки, другие файлы, из которых собрана сборка. По
    /// содержимому файла и этому отпечатку программа хранит готовый STEP на
    /// складе и второй раз CAD-систему не будит. `None` — условия узнать
    /// нельзя: такой STEP на склад не кладётся.
    ///
    /// По умолчанию — модуль и его версия: годится модулю, чей ответ
    /// зависит только от самого файла.
    fn stamp(&self, source: &Path) -> Option<String> {
        let _ = source;
        let manifest = self.manifest();
        Some(format!("{} {}", manifest.id, manifest.version))
    }
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

/// Беглый взгляд на форму: контрольные числа прямо из родного файла CAD,
/// без CAD-системы и без STEP. Нужен там, где файлов много и ждать нельзя:
/// разбор папки сверяет формой десятки деталей подряд. Числа приблизительные
/// (обычно их считают по сетке показа, сохранённой в файле), поэтому в
/// паспорт версии они не идут и с точными числами из STEP на равных не
/// сравниваются.
pub trait ShapeGlance: Module {
    /// Берётся ли модуль за этот файл (обычно по расширению).
    fn can_glance(&self, path: &Path) -> bool;
    /// Объём, площадь, габариты и число граней. Ошибка — когда в файле нет
    /// нужных данных или они устарели: тогда форму узнают обычным путём.
    /// Это строгий ответ: числам можно верить целиком.
    fn glance(&self, path: &Path) -> Result<GeometrySummary>;

    /// Третий исход: «прочитано частично, вот что не прочитано». Модуль
    /// отдаёт все числа, какие смог снять, а рядом — список пропусков:
    /// например, габариты и площадь есть, а объёма нет, потому что сетка
    /// не замкнута. Ошибка — только когда снять нечего.
    ///
    /// Если строгий [`glance`](Self::glance) ответил, здесь те же числа; если
    /// он отказал, а здесь числа есть, — причина отказа записана пропуском.
    /// По умолчанию — строгий ответ без пропусков.
    fn glance_partial(&self, path: &Path) -> Result<Partial<GeometrySummary>> {
        self.glance(path).map(Partial::whole)
    }
}

/// Замысел конструктора прямо из родного файла CAD, без CAD-системы:
/// список операций с постоянными идентификаторами и размерами, свойства,
/// признаки «спорной» детали. По нему строят слова «что изменилось» не по
/// геометрии и решают, стоит ли будить CAD-систему ради точной формы.
///
/// От [`TreeSource`] отличается тем, что читает файл как есть: порядок
/// дерева и погашение ему неизвестны, размеры — как показаны на экране, а
/// ответ бывает неполным ([`Partial`]). В `tree.yaml` его ответ не пишется.
pub trait DesignSource: Module {
    /// Умеет ли модуль читать этот файл (обычно по расширению).
    fn can_read(&self, path: &Path) -> bool;
    /// Прочитать замысел. Что не прочиталось — в списке пропусков; ошибка —
    /// только когда файл не открылся вовсе.
    fn read_design(&self, path: &Path) -> Result<Partial<Design>>;
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
