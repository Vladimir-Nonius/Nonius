//! Окошко «Добавить модель» на каждый файл: что это за деталь и что изменилось.
//! Модель сразу сохраняется версией, поэтому второй файл той же детали — это
//! v2 и его можно сравнить с первым. Сюда приходят меню, перетаскивание и
//! «Заменить модель…».

use std::collections::VecDeque;
use std::path::PathBuf;

use eframe::egui;
use nonius_core::PartName;

mod dialog;

pub use dialog::show;

use super::jobs::Done;
use super::model_names::{default_message, guess, same_name, Place};
use super::App;
use crate::source::{AddedModel, PartEntry};
use crate::texts::texts;

/// Открытое окошко.
#[derive(Debug, Default)]
pub struct AddModel {
    /// Файлы; первый — на экране.
    queue: VecDeque<PathBuf>,
    /// Сколько файлов было всего.
    total: usize,
    /// Куда добавлять; `None` — новая деталь с именем `name`.
    target: Option<String>,
    /// Деталь задана заранее: «Заменить модель…».
    fixed: bool,
    /// Файлы бросили на эту папку: новая деталь ляжет в неё. `None` —
    /// в корень, как раньше.
    folder: Option<String>,
    name: String,
    message: String,
    busy: bool,
    /// Добавить все оставшиеся файлы без вопросов.
    all: bool,
    error: Option<String>,
    focused: bool,
    /// Что получилось с уже добавленными файлами.
    results: Vec<AddedModel>,
}

impl AddModel {
    fn file(&self) -> Option<&PathBuf> {
        self.queue.front()
    }

    /// Показать первый файл очереди: имя из файла, деталь угадать.
    fn prepare(&mut self, parts: &[PartEntry]) {
        let Some(file) = self.file() else {
            return;
        };
        let stem = file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.name = PartName::cleaned(&stem).map_or(stem, |n| n.to_string());
        if !self.fixed {
            self.target = guess(&self.name, parts).map(|p| p.name.clone());
        }
        if !self.all {
            self.message = default_message(self.target.as_deref(), parts);
        }
        self.focused = false;
        self.error = None;
    }

    /// Куда добавится файл при таком выборе.
    fn place(&self, parts: &[PartEntry]) -> Place {
        if let Some(part) = &self.target {
            return Place::Existing(part.clone());
        }
        match PartName::cleaned(&self.name) {
            Some(name) => match same_name(name.as_str(), parts) {
                Some(p) => Place::Existing(p.name.clone()),
                None => Place::New(name.to_string()),
            },
            None => Place::Nothing,
        }
    }

    /// Файл добавлен: дальше следующий. Возвращает, есть ли он.
    fn advance(&mut self, added: AddedModel, parts: &[PartEntry]) -> bool {
        self.busy = false;
        self.queue.pop_front();
        self.results.push(added);
        self.prepare(parts);
        !self.queue.is_empty()
    }

    /// Не получилось: окошко остаётся, причина видна в нём.
    pub fn failed(&mut self, error: String) {
        self.busy = false;
        self.all = false;
        self.error = Some(error);
    }

    /// Что сказать, когда всё добавлено, и какую деталь показать.
    fn outcome(&self) -> Option<(String, String)> {
        let t = texts();
        let last = self.results.last()?;
        let mut text = match (self.results.as_slice(), last.version) {
            ([_], Some(v)) => format!("{} «{}» v{v}", t.model_added, last.part),
            ([_], None) => format!("«{}»: {}", last.part, t.model_same),
            (list, _) => format!("{}: {}", t.models_added, list.len()),
        };
        let warnings: Vec<&str> = self
            .results
            .iter()
            .flat_map(|r| r.warnings.iter().map(String::as_str))
            .collect();
        if !warnings.is_empty() {
            text = format!("{text}. {}", warnings.join("; "));
        }
        Some((last.part.clone(), text))
    }
}

impl App {
    /// Начать добавлять файлы. `part` — положить в эту деталь. Окошко уже
    /// открыто — файлы встают в очередь.
    pub(super) fn start_adding(&mut self, files: Vec<PathBuf>, part: Option<String>) {
        if files.is_empty() {
            return;
        }
        if let Some(d) = self.tab.add.as_mut() {
            d.total += files.len();
            d.queue.extend(files);
            return;
        }
        let mut dialog = AddModel {
            total: files.len(),
            queue: files.into(),
            fixed: part.is_some(),
            target: part,
            ..AddModel::default()
        };
        dialog.prepare(&self.tab.parts);
        self.tab.add = Some(dialog);
    }

    /// Модели бросили на папку: новые детали из них лягут в неё.
    pub(super) fn start_adding_in(&mut self, files: Vec<PathBuf>, folder: String) {
        let fresh = self.tab.add.is_none();
        self.start_adding(files, None);
        if let (true, Some(d)) = (fresh, self.tab.add.as_mut()) {
            d.folder = Some(folder);
        }
    }

    /// Добавить файл на экране и сохранить версию.
    fn add_current(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(d)) = (self.tab.project.clone(), self.tab.add.as_mut()) else {
            return;
        };
        let (Some(file), part) = (d.file().cloned(), d.place(&self.tab.parts)) else {
            return;
        };
        // Новая деталь появляется в корне: потом её переложить в папку.
        let folder = match &part {
            Place::New(_) => d.folder.clone().filter(|f| !f.is_empty()),
            _ => None,
        };
        let (Place::Existing(part) | Place::New(part)) = part else {
            d.failed(texts().bad_name.to_string());
            return;
        };
        d.busy = true;
        d.error = None;
        let message = d.message.trim().to_string();
        self.tab.jobs.cancel();
        self.tab.jobs.spawn(ctx, move |progress| {
            progress.step(texts().importing);
            let mut added = project.add_model(&file, &part, &message);
            if let (Ok(a), Some(folder)) = (added.as_mut(), folder) {
                // Модель уже сохранена; не легла в папку — сказать, но не
                // терять.
                if let Err(e) = project.move_part(&part, &folder) {
                    a.warnings.push(format!("«{folder}»: {e:#}"));
                }
            }
            Done::Added(added)
        });
    }

    /// Файл добавился: следующий или конец.
    pub(super) fn added(&mut self, ctx: &egui::Context, added: AddedModel) {
        // Деталь уже есть для следующего файла, хотя список ещё не перечитан.
        let version = added.version.map(|v| format!("v{v}"));
        let folder = self.tab.add.as_ref().and_then(|d| d.folder.clone());
        match self.tab.parts.iter_mut().find(|p| p.name == added.part) {
            Some(p) => p.latest = version.or(p.latest.take()),
            None => self.tab.parts.push(PartEntry {
                name: added.part.clone(),
                assembly: false,
                latest: version,
                dirty: false,
                model: None,
                stamp: String::new(),
                folder: folder.unwrap_or_default(),
            }),
        }
        let parts = self.tab.parts.clone();
        let Some(d) = self.tab.add.as_mut() else {
            return;
        };
        if !d.advance(added, &parts) {
            self.finish_adding(ctx);
        } else if d.all {
            self.add_current(ctx);
        }
    }

    /// Закрыть окошко и показать, что получилось.
    fn finish_adding(&mut self, ctx: &egui::Context) {
        let outcome = self.tab.add.take().and_then(|d| d.outcome());
        if let Some((part, text)) = outcome {
            self.reload_on(ctx, part, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_name_of_an_existing_part_means_a_new_version() {
        let parts = [PartEntry {
            name: "кронштейн".into(),
            assembly: false,
            latest: Some("v2".into()),
            dirty: false,
            model: None,
            stamp: String::new(),
            folder: String::new(),
        }];
        let mut d = AddModel {
            name: "КРОНШТЕЙН".into(),
            ..AddModel::default()
        };
        assert_eq!(d.place(&parts), Place::Existing("кронштейн".into()));
        d.name = "плита #2".into();
        assert_eq!(d.place(&parts), Place::New("плита _2".into()));
        d.name = "...".into();
        assert_eq!(d.place(&parts), Place::Nothing);
    }
}
