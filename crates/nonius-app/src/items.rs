//! Детали и сборки проекта: создать, список, статус (новая, изменена, сохранена).

use nonius_core::meta::ItemKind;
use nonius_core::project::Item;
use nonius_core::PartName;
use nonius_store::{DeletedItem, FileChange, VersionInfo};

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
    /// Сколько всего версий.
    pub versions: usize,
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

    /// Удалить деталь. Файлы и версии остаются в истории, несохранённое
    /// сохраняется версией: деталь можно вернуть.
    pub fn delete_item(&self, name: &str) -> Result<()> {
        Ok(self.store()?.delete(&self.item(name)?)?)
    }

    /// Удалённые детали, которые можно вернуть, новые первыми.
    pub fn deleted_items(&self) -> Result<Vec<DeletedItem>> {
        Ok(self.store()?.deleted()?)
    }

    /// Вернуть удалённую деталь такой, какой она была перед удалением.
    pub fn recover_item(&self, name: &str) -> Result<Item> {
        Ok(self.store()?.recover(&PartName::new(name)?)?)
    }

    /// Переименовать деталь. Её версии остаются при ней; несохранённое надо
    /// сначала сохранить.
    pub fn rename_item(&self, name: &str, new: &str) -> Result<Item> {
        let item = self.item(name)?;
        Ok(self.store()?.rename(&item, &PartName::new(new)?)?)
    }

    /// Детали и сборки без статуса: быстро и без git.
    pub fn item_list(&self) -> Result<Vec<Item>> {
        Ok(self.project.items()?)
    }

    /// Все детали и сборки со статусом. git запускается дважды на весь
    /// проект, а не на каждую деталь.
    pub fn items(&self) -> Result<Vec<ItemStatus>> {
        let items = self.item_list()?;
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let histories = self.store()?.histories(&items)?;
        Ok(items
            .into_iter()
            .zip(histories)
            .map(|(item, h)| ItemStatus {
                latest: h.versions.first().cloned(),
                versions: h.versions.len(),
                changes: h.changes,
                item,
            })
            .collect())
    }

    /// Статус одной детали.
    pub fn item_status(&self, name: &str) -> Result<ItemStatus> {
        self.status_of(self.item(name)?)
    }

    /// Несохранённые изменения детали — без истории, быстрее полного статуса.
    pub fn changes(&self, item: &Item) -> Result<Vec<FileChange>> {
        Ok(self.store()?.changes(item)?)
    }

    /// Последняя версия детали.
    pub fn latest(&self, item: &Item) -> Result<Option<VersionInfo>> {
        Ok(self.store()?.latest(item)?)
    }

    fn status_of(&self, item: Item) -> Result<ItemStatus> {
        let versions = self.store()?.versions(&item)?;
        Ok(ItemStatus {
            latest: versions.first().cloned(),
            versions: versions.len(),
            changes: self.changes(&item)?,
            item,
        })
    }
}

impl App {
    /// Отпечаток модели и паспорта детали: окно замечает правки извне.
    pub fn stamp(&self, item: &Item) -> String {
        crate::cache::model_fingerprint(&item.dir, true).unwrap_or_default()
    }
}
