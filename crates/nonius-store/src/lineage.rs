//! Чья какая версия. Версия подписана именем детали на момент сохранения, а
//! деталь могли с тех пор переименовать, удалить или создать заново под тем же
//! именем. История идёт от новых записей к старым, поэтому имя можно
//! проследить назад: на отметке «переименована» оно меняется на прежнее, на
//! отметке «удалена» история нынешней детали заканчивается. Отметка «убрана с
//! карты» относится к детали под тем же прослеженным именем.

use std::collections::{HashMap, HashSet};

use nonius_core::project::Item;
use nonius_core::VersionNumber;

use crate::git::git_path;
use crate::message::{parse_event, Event, LOG_EVENTS, LOG_FORMAT};
use crate::{DeletedItem, Result, Store, VersionInfo};

impl Store {
    /// Записи истории проекта, новые первыми.
    fn events(&self) -> Result<Vec<Event>> {
        let mut args = vec!["log", "-z", LOG_FORMAT];
        args.extend(LOG_EVENTS);
        let log = self.git.run(&args)?;
        Ok(String::from_utf8_lossy(&log)
            .split('\0')
            .filter_map(parse_event)
            .collect())
    }

    /// Версии каждой детали из `items`, в том же порядке, новые первыми.
    pub(crate) fn versions_of(&self, items: &[Item]) -> Result<Vec<Vec<VersionInfo>>> {
        let mut list = vec![Vec::new(); items.len()];
        let mut owner: HashMap<String, usize> = items
            .iter()
            .enumerate()
            .map(|(i, item)| (item.name.to_string(), i))
            .collect();
        // Вернули удалённую: всё до отметки удаления — уже не про неё.
        let mut recovered = HashMap::new();
        // Убрана ли версия с карты: решает самая новая отметка.
        let mut marks: HashMap<(usize, VersionNumber), bool> = HashMap::new();
        for event in self.events()? {
            match event {
                Event::Version(mut v) => {
                    if let Some(&i) = owner.get(v.item.as_str()) {
                        let dir = self.project.item_dir(items[i].kind, &v.item);
                        v.folder = git_path(self.project.root(), &dir);
                        v.item = items[i].name.clone();
                        list[i].push(v);
                    }
                }
                Event::Renamed { from, to } => match owner.remove(to.as_str()) {
                    Some(i) => {
                        owner.insert(from.to_string(), i);
                    }
                    // Старые версии с этим именем — у той детали, что ушла.
                    None => {
                        owner.remove(from.as_str());
                    }
                },
                Event::Deleted(d) => match recovered.remove(d.name.as_str()) {
                    Some(i) => {
                        owner.insert(d.name.to_string(), i);
                    }
                    None => {
                        owner.remove(d.name.as_str());
                    }
                },
                Event::Recovered(name) => {
                    if let Some(i) = owner.remove(name.as_str()) {
                        recovered.insert(name.to_string(), i);
                    }
                }
                Event::Hidden {
                    item,
                    version,
                    hidden,
                } => {
                    if let Some(&i) = owner.get(item.as_str()) {
                        marks.entry((i, version)).or_insert(hidden);
                    }
                }
            }
        }
        for (i, versions) in list.iter_mut().enumerate() {
            for v in versions {
                v.hidden = marks.get(&(i, v.version)).copied().unwrap_or(false);
            }
        }
        Ok(list)
    }

    /// Удалённые детали, которые ещё не вернули, новые первыми. Деталь,
    /// удалённая дважды, идёт один раз: вернуть можно последнее.
    pub fn deleted(&self) -> Result<Vec<DeletedItem>> {
        let mut recovered = HashSet::new();
        let mut seen = HashSet::new();
        let mut list = Vec::new();
        for event in self.events()? {
            match event {
                Event::Recovered(name) => {
                    recovered.insert(name.to_string());
                }
                Event::Deleted(d)
                    if !recovered.remove(d.name.as_str()) && seen.insert(d.name.to_string()) =>
                {
                    list.push(d);
                }
                _ => {}
            }
        }
        Ok(list)
    }
}
