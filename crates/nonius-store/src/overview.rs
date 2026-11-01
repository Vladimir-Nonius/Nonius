//! История и изменения всех деталей за один проход: `git log` и `git status`
//! запускаются на весь проект, а не на каждую деталь. На сотнях деталей это
//! разница между долями секунды и десятками секунд.

use std::collections::{BTreeMap, HashMap};

use nonius_core::project::Item;

use crate::history::parse_status;
use crate::message::{parse_any_record, LOG_FORMAT};
use crate::{FileChange, Result, Store, VersionInfo};

/// Что известно о детали без чтения её файлов.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemHistory {
    /// Версии, новые первыми.
    pub versions: Vec<VersionInfo>,
    /// Изменённые файлы с последней версии.
    pub changes: Vec<FileChange>,
}

impl Store {
    /// История каждой детали из `items`, в том же порядке.
    pub fn histories(&self, items: &[Item]) -> Result<Vec<ItemHistory>> {
        let mut list = vec![ItemHistory::default(); items.len()];
        let by_name: BTreeMap<&str, usize> = items
            .iter()
            .enumerate()
            .map(|(i, item)| (item.name.as_str(), i))
            .collect();
        let log = self
            .git
            .run(&["log", "-z", LOG_FORMAT, "--grep=^Nonius-Version: "])?;
        for record in String::from_utf8_lossy(&log).split('\0') {
            let Some(version) = parse_any_record(record) else {
                continue;
            };
            if let Some(&i) = by_name.get(version.item.as_str()) {
                list[i].versions.push(version);
            }
        }

        let paths: Vec<String> = items.iter().map(|item| self.pathspec(item)).collect();
        let by_dir: HashMap<&str, usize> = paths
            .iter()
            .enumerate()
            .map(|(i, path)| (path.as_str(), i))
            .collect();
        // Без переименований: файл, перенесённый между деталями, изменил обе.
        let status = self.git.run(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ])?;
        for mut change in parse_status(&status) {
            let owner = change
                .path
                .match_indices('/')
                .find_map(|(end, _)| Some((end, *by_dir.get(&change.path[..end])?)));
            if let Some((end, i)) = owner {
                change.path.drain(..=end);
                list[i].changes.push(change);
            }
        }
        for history in &mut list {
            history.changes.sort_by(|a, b| a.path.cmp(&b.path));
        }
        Ok(list)
    }
}
