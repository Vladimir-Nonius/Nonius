//! Реестр: подключает модули к точкам расширения и проверяет их манифесты.

use std::path::Path;

use super::{
    Comparator, Event, EventHandler, Exporter, ExtensionPoint, GeometrySource, Module,
    ModuleManifest, Renderer, Snapshot, TreeSource,
};
use crate::compare::{ChangeReport, CompareContext};
use crate::error::{Error, Result};

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
