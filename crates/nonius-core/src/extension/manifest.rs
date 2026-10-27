//! Манифест модуля: id, версия, версия API, точки расширения, права.

use serde::{Deserialize, Serialize};

/// Текущая версия API модулей.
pub const API_VERSION: u32 = 1;
/// Самая старая версия API, которую ядро ещё поддерживает.
pub const MIN_API_VERSION: u32 = 1;

/// Точка расширения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExtensionPoint {
    /// Файл → геометрия и метаданные ([`GeometrySource`](super::GeometrySource)).
    GeometrySource,
    /// Файл → дерево построения ([`TreeSource`](super::TreeSource)).
    TreeSource,
    /// Различия двух версий ([`Comparator`](super::Comparator)).
    Comparator,
    /// Картинки и анимации изменений ([`Renderer`](super::Renderer)).
    Renderer,
    /// Файлы из версии: DXF, PDF, STL ([`Exporter`](super::Exporter)).
    Exporter,
    /// Реакция на события проекта ([`EventHandler`](super::EventHandler)).
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
