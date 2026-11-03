//! Папки проекта: карта «что где лежит», папки-группы, перенос детали и
//! перевод старого проекта (`parts/`, `assemblies/`) на папки.

use std::path::{Path, PathBuf};

use nonius_core::meta::ItemKind;
use nonius_core::project::Item;
use nonius_core::PartName;

use crate::{App, Result};

/// Карта проекта: папки и где лежит каждая деталь. Это просто обход диска,
/// отдельно она нигде не хранится.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectMap {
    /// Папки-группы, путь от корня через `/`, родитель раньше детей.
    pub folders: Vec<String>,
    /// Детали и сборки по порядку папок.
    pub items: Vec<MapItem>,
}

/// Деталь на карте проекта.
#[derive(Debug, Clone, PartialEq)]
pub struct MapItem {
    /// Имя.
    pub name: PartName,
    /// Деталь или сборка.
    pub kind: ItemKind,
    /// Папка детали, путь от корня через `/`.
    pub path: String,
}

impl App {
    /// Проект старого устройства: детали в `parts/` и `assemblies/`, менять
    /// ничего нельзя, пока его не обновят ([`App::upgrade`]).
    pub fn is_old_layout(&self) -> bool {
        self.project.is_old_layout()
    }

    /// Перевести старый проект на папки: детали и сборки переезжают в корень
    /// одной записью истории. После этого проект надо открыть заново.
    pub fn upgrade(&self) -> Result<()> {
        self.store()?.upgrade()?;
        Ok(())
    }

    /// Карта проекта.
    pub fn map(&self) -> Result<ProjectMap> {
        let layout = self.project.scan()?;
        let root = self.project.root();
        let items = layout
            .items
            .into_iter()
            .map(|item| MapItem {
                path: rel_path(root, &item.dir),
                name: item.name,
                kind: item.kind,
            })
            .collect();
        Ok(ProjectMap {
            folders: layout.folders,
            items,
        })
    }

    /// Создать папку-группу (путь от корня, можно сразу несколько уровней).
    pub fn new_folder(&self, folder: &Path) -> Result<PathBuf> {
        Ok(self.project.create_folder(folder)?)
    }

    /// Добавить деталь или сборку в папку (путь от корня, пустой — корень).
    pub fn new_item_in(&self, name: &str, kind: ItemKind, folder: &Path) -> Result<Item> {
        Ok(self
            .project
            .create_item_in(kind, &PartName::new(name)?, folder)?)
    }

    /// Перенести деталь в папку (путь от корня, пустой — корень). Версии
    /// остаются при ней; несохранённое надо сначала сохранить.
    pub fn move_item(&self, name: &str, folder: &Path) -> Result<Item> {
        let item = self.item(name)?;
        Ok(self.store()?.move_item(&item, folder)?)
    }
}

/// Путь от корня проекта через `/`.
fn rel_path(root: &Path, dir: &Path) -> String {
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
