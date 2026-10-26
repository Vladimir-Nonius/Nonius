//! Точки расширения: как модули подключаются к ядру.
//!
//! Принцип — маленькое стабильное ядро, всё остальное модули. Встроенные
//! функции (импорт STEP, подсветка изменений) подключаются через эти же
//! трейты, что и сторонние модули, поэтому интерфейсы проверяются с первого дня.
//!
//! Правила, чтобы модули ничего не ломали:
//! - у каждого модуля [`ModuleManifest`]: имя, версия, API, точки, права;
//! - модуль подключается только к точкам, объявленным в манифесте;
//! - API модулей версионирован ([`API_VERSION`]);
//! - для каждой точки есть контрактные тесты в [`crate::contract`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::compare::{ChangeReport, CompareContext};
use crate::error::{Error, Result};
use crate::meta::PartMeta;
use crate::names::{PartName, VersionNumber};
use crate::tree::FeatureTree;

/// Текущая версия API модулей.
pub const API_VERSION: u32 = 1;
/// Самая старая версия API, которую ядро ещё поддерживает.
pub const MIN_API_VERSION: u32 = 1;

/// Точка расширения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExtensionPoint {
    /// Файл → геометрия и метаданные ([`GeometrySource`]).
    GeometrySource,
    /// Файл → дерево построения ([`TreeSource`]).
    TreeSource,
    /// Различия двух версий ([`Comparator`]).
    Comparator,
    /// Картинки и анимации изменений ([`Renderer`]).
    Renderer,
    /// Файлы из версии: DXF, PDF, STL ([`Exporter`]).
    Exporter,
    /// Реакция на события проекта ([`EventHandler`]).
    Events,
    /// Панели и вкладки интерфейса (подключаются веб-интерфейсом).
    Ui,
}

/// Право, которое модуль запрашивает.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Permission {
    /// Читать файлы проекта.
    ReadProject,
    /// Писать файлы проекта.
    WriteProject,
    /// Ходить в сеть.
    Network,
}

/// Манифест модуля.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleManifest {
    /// Идентификатор: строчные латинские буквы, цифры и дефис (`step-occt`).
    pub id: String,
    /// Название для человека.
    pub name: String,
    /// Версия модуля.
    pub version: String,
    /// Версия API модулей, под которую написан модуль.
    pub api: u32,
    /// Точки расширения, к которым модуль подключается.
    pub points: Vec<ExtensionPoint>,
    /// Права, которые модуль запрашивает.
    #[serde(default)]
    pub permissions: Vec<Permission>,
}

impl ModuleManifest {
    /// Проверить манифест. Возвращает список нарушений (пустой — всё хорошо).
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        let id_ok = !self.id.is_empty()
            && !self.id.starts_with('-')
            && !self.id.ends_with('-')
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !id_ok {
            p.push(format!(
                "id «{}»: нужны строчные латинские буквы, цифры и дефис",
                self.id
            ));
        }
        if self.name.trim().is_empty() {
            p.push("пустое название".into());
        }
        if self.version.trim().is_empty() {
            p.push("пустая версия".into());
        }
        if !(MIN_API_VERSION..=API_VERSION).contains(&self.api) {
            p.push(format!(
                "API {} не поддерживается (поддерживаются {MIN_API_VERSION}..={API_VERSION})",
                self.api
            ));
        }
        if self.points.is_empty() {
            p.push("не объявлено ни одной точки расширения".into());
        }
        for (i, point) in self.points.iter().enumerate() {
            if self.points[..i].contains(point) {
                p.push(format!("точка {point:?} объявлена дважды"));
            }
        }
        p
    }
}

/// Общая часть всех модулей.
pub trait Module: Send + Sync {
    /// Манифест модуля.
    fn manifest(&self) -> &ModuleManifest;
}

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
}

/// Экспорт файлов из версии.
pub trait Exporter: Module {
    /// Форматы, которые модуль умеет выдавать: `dxf`, `pdf`, `stl`.
    fn formats(&self) -> Vec<String>;
    /// Выгрузить версию в формате `format` в папку `out_dir`.
    fn export(&self, snapshot: &Snapshot, format: &str, out_dir: &Path) -> Result<PathBuf>;
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

/// Реестр подключённых модулей.
#[derive(Default)]
pub struct Registry {
    geometry: Vec<Box<dyn GeometrySource>>,
    trees: Vec<Box<dyn TreeSource>>,
    comparators: Vec<Box<dyn Comparator>>,
    renderers: Vec<Box<dyn Renderer>>,
    exporters: Vec<Box<dyn Exporter>>,
    events: Vec<Box<dyn EventHandler>>,
}

impl Registry {
    /// Пустой реестр.
    pub fn new() -> Self {
        Self::default()
    }

    /// Реестр со встроенными модулями ядра.
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        r.add_comparator(Box::new(crate::compare::MetaComparator::new()))
            .expect("встроенный модуль должен проходить проверку");
        r
    }

    /// Подключить источник геометрии.
    pub fn add_geometry_source(&mut self, m: Box<dyn GeometrySource>) -> Result<()> {
        check(
            m.manifest(),
            ExtensionPoint::GeometrySource,
            ids(&self.geometry),
        )?;
        self.geometry.push(m);
        Ok(())
    }

    /// Подключить источник дерева.
    pub fn add_tree_source(&mut self, m: Box<dyn TreeSource>) -> Result<()> {
        check(m.manifest(), ExtensionPoint::TreeSource, ids(&self.trees))?;
        self.trees.push(m);
        Ok(())
    }

    /// Подключить сравнение.
    pub fn add_comparator(&mut self, m: Box<dyn Comparator>) -> Result<()> {
        check(
            m.manifest(),
            ExtensionPoint::Comparator,
            ids(&self.comparators),
        )?;
        self.comparators.push(m);
        Ok(())
    }

    /// Подключить отрисовку.
    pub fn add_renderer(&mut self, m: Box<dyn Renderer>) -> Result<()> {
        check(m.manifest(), ExtensionPoint::Renderer, ids(&self.renderers))?;
        self.renderers.push(m);
        Ok(())
    }

    /// Подключить экспорт.
    pub fn add_exporter(&mut self, m: Box<dyn Exporter>) -> Result<()> {
        check(m.manifest(), ExtensionPoint::Exporter, ids(&self.exporters))?;
        self.exporters.push(m);
        Ok(())
    }

    /// Подключить обработчик событий.
    pub fn add_event_handler(&mut self, m: Box<dyn EventHandler>) -> Result<()> {
        check(m.manifest(), ExtensionPoint::Events, ids(&self.events))?;
        self.events.push(m);
        Ok(())
    }

    /// Первый источник геометрии, который умеет читать файл.
    pub fn geometry_source_for(&self, path: &Path) -> Option<&dyn GeometrySource> {
        self.geometry
            .iter()
            .find(|m| m.can_read(path))
            .map(|m| m.as_ref())
    }

    /// Первый источник дерева, который умеет читать файл.
    pub fn tree_source_for(&self, path: &Path) -> Option<&dyn TreeSource> {
        self.trees
            .iter()
            .find(|m| m.can_read(path))
            .map(|m| m.as_ref())
    }

    /// Прогнать все сравнения. Отчёты с изменениями идут первыми не обязательно:
    /// порядок — порядок подключения модулей.
    pub fn compare_all(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        ctx: &CompareContext,
    ) -> Result<Vec<ChangeReport>> {
        self.comparators
            .iter()
            .map(|c| c.compare(before, after, ctx))
            .collect()
    }

    /// Разослать событие всем обработчикам. Ошибки обработчиков собираются,
    /// но не прерывают рассылку.
    pub fn emit(&self, event: &Event) -> Vec<Error> {
        self.events
            .iter()
            .filter_map(|h| h.on_event(event).err())
            .collect()
    }

    /// Экспорт, который умеет формат.
    pub fn exporter_for(&self, format: &str) -> Option<&dyn Exporter> {
        self.exporters
            .iter()
            .find(|e| e.formats().iter().any(|f| f.eq_ignore_ascii_case(format)))
            .map(|m| m.as_ref())
    }

    /// Все подключённые отрисовщики.
    pub fn renderers(&self) -> impl Iterator<Item = &dyn Renderer> + '_ {
        self.renderers.iter().map(|m| m.as_ref())
    }

    /// Манифесты всех подключённых модулей (модуль с несколькими точками — один раз).
    pub fn manifests(&self) -> Vec<&ModuleManifest> {
        let mut all: Vec<&ModuleManifest> = Vec::new();
        let lists: [Vec<&ModuleManifest>; 6] = [
            self.geometry.iter().map(|m| m.manifest()).collect(),
            self.trees.iter().map(|m| m.manifest()).collect(),
            self.comparators.iter().map(|m| m.manifest()).collect(),
            self.renderers.iter().map(|m| m.manifest()).collect(),
            self.exporters.iter().map(|m| m.manifest()).collect(),
            self.events.iter().map(|m| m.manifest()).collect(),
        ];
        for m in lists.into_iter().flatten() {
            if !all.iter().any(|x| x.id == m.id) {
                all.push(m);
            }
        }
        all
    }
}

fn ids<M: Module + ?Sized>(list: &[Box<M>]) -> Vec<&str> {
    list.iter().map(|m| m.manifest().id.as_str()).collect()
}

fn check(m: &ModuleManifest, point: ExtensionPoint, already: Vec<&str>) -> Result<()> {
    let problems = m.problems();
    if !problems.is_empty() {
        return Err(Error::module(&m.id, problems.join("; ")));
    }
    if !m.points.contains(&point) {
        return Err(Error::module(
            &m.id,
            format!("подключается к {point:?}, но не объявил эту точку в манифесте"),
        ));
    }
    if already.contains(&m.id.as_str()) {
        return Err(Error::module(&m.id, format!("уже подключён к {point:?}")));
    }
    Ok(())
}
