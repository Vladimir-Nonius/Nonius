//! Детали и сборки проекта: создать, список, статус (новая, изменена, сохранена).

use nonius_core::meta::ItemKind;
use nonius_core::project::Item;
use nonius_core::PartName;
use nonius_store::{FileChange, VersionInfo};

use crate::{App, Result};

/// Состояние детали относительно истории.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    /// Версий ещё нет.
    New,
    /// Есть изменения, которые ещё не сохранены версией.
    Changed,
    /// Всё сохранено.
    Saved,
}

/// Деталь со статусом: последняя версия и несохранённые изменения.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStatus {
    /// Деталь или сборка.
    pub item: Item,
    /// Последняя версия, если есть.
    pub latest: Option<VersionInfo>,
    /// Изменённые файлы с последней версии.
    pub changes: Vec<FileChange>,
}

impl ItemStatus {
    /// Новая, изменена или сохранена.
    pub fn state(&self) -> ItemState {
        match (&self.latest, self.changes.is_empty()) {
            (None, _) => ItemState::New,
            (Some(_), false) => ItemState::Changed,
            (Some(_), true) => ItemState::Saved,
        }
    }
}

impl App {
    /// Добавить деталь или сборку.
    pub fn new_item(&self, name: &str, kind: ItemKind) -> Result<Item> {
        Ok(self.project.create_item(kind, &PartName::new(name)?)?)
    }

    /// Детали и сборки без статуса: быстро и без git.
    pub fn item_list(&self) -> Result<Vec<Item>> {
        Ok(self.project.items()?)
    }

    /// Все детали и сборки со статусом.
    pub fn items(&self) -> Result<Vec<ItemStatus>> {
        self.item_list()?
            .into_iter()
            .map(|i| self.status_of(i))
            .collect()
    }

    /// Статус одной детали.
    pub fn item_status(&self, name: &str) -> Result<ItemStatus> {
        self.status_of(self.item(name)?)
    }

    fn status_of(&self, item: Item) -> Result<ItemStatus> {
        let store = self.store()?;
        Ok(ItemStatus {
            latest: store.latest(&item)?,
            changes: store.changes(&item)?,
            item,
        })
    }
}
