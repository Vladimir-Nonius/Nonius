//! Окошко «Добавить модель» на каждый файл: что это за деталь и что изменилось.
//! Модель сразу сохраняется версией, поэтому второй файл той же детали — это
//! v2 и его можно сравнить с первым. Сюда приходят меню, перетаскивание и
//! «Заменить модель…».

use std::collections::VecDeque;
use std::path::PathBuf;

use eframe::egui::{self, RichText};
use nonius_core::PartName;

use super::jobs::Done;
use super::model_names::{default_message, guess, next_version, same_name, step_stays, Place};
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

/// Нарисовать окошко, если оно открыто.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let parts = app.parts.clone();
    let Some(d) = app.add.as_mut() else {
        return;
    };
    let Some(file) = d.file().cloned() else {
        return;
    };
    let mut add = false;
    let mut all = false;
    let mut skip = false;
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("add-model")).show(ctx, |ui| {
        ui.set_width(460.0);
        let done = d.total - d.queue.len() + 1;
        ui.heading(if d.total > 1 {
            format!("{} {done} {} {}", t.add_model_title, t.place_of, d.total)
        } else {
            t.add_model_title.to_string()
        });
        let shown = file.file_name().unwrap_or_default().to_string_lossy();
        ui.label(RichText::new(shown).weak());
        ui.add_space(6.0);
        if !d.fixed {
            let before = d.target.clone();
            let selected = d.target.as_deref().unwrap_or(t.part_new);
            ui.label(t.add_to);
            egui::ComboBox::from_id_salt("add-target")
                .selected_text(selected)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut d.target, None, t.part_new);
                    for p in &parts {
                        ui.selectable_value(&mut d.target, Some(p.name.clone()), &p.name);
                    }
                });
            if d.target != before {
                d.message = default_message(d.target.as_deref(), &parts);
            }
        }
        if d.target.is_none() {
            ui.label(t.part_name);
            ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(f32::INFINITY));
        }
        let place = d.place(&parts);
        match &place {
            Place::Existing(part) => {
                let next = next_version(part, &parts);
                ui.label(RichText::new(format!("{} {next} «{part}»", t.will_be_version)).strong());
            }
            Place::New(name) => {
                ui.label(RichText::new(format!("{} «{name}»", t.will_be_part)).strong());
            }
            Place::Nothing => {
                ui.label(RichText::new(t.bad_name).color(ui.visuals().warn_fg_color));
            }
        }
        if step_stays(&file, &place, &parts) {
            ui.label(RichText::new(t.step_stays).color(ui.visuals().warn_fg_color));
        }
        ui.add_space(6.0);
        ui.label(format!("{} *", t.what_why));
        let edit = ui.add(
            egui::TextEdit::singleline(&mut d.message)
                .desired_width(f32::INFINITY)
                .hint_text(t.add_model_hint),
        );
        if !d.focused {
            edit.request_focus();
            d.focused = true;
        }
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if let Some(e) = &d.error {
            ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
        }
        ui.add_space(6.0);
        let ready = !d.busy && place != Place::Nothing && !d.message.trim().is_empty();
        ui.horizontal(|ui| {
            let label = if d.busy { t.importing } else { t.add };
            let button = ui.add_enabled(ready, egui::Button::new(label));
            add = ready && (button.clicked() || enter);
            if d.queue.len() > 1 {
                let label = format!("{} ({})", t.add_all, d.queue.len());
                all = ui.add_enabled(ready, egui::Button::new(label)).clicked();
                skip = ui.add_enabled(!d.busy, egui::Button::new(t.skip)).clicked();
            }
            close = ui
                .add_enabled(!d.busy, egui::Button::new(t.cancel))
                .clicked();
        });
    });
    if close || (modal.should_close() && !d.busy) {
        app.finish_adding(ctx);
    } else if skip {
        d.queue.pop_front();
        d.prepare(&parts);
    } else if add || all {
        d.all = all;
        app.add_current(ctx);
    }
}

impl App {
    /// Начать добавлять файлы. `part` — положить в эту деталь. Окошко уже
    /// открыто — файлы встают в очередь.
    pub(super) fn start_adding(&mut self, files: Vec<PathBuf>, part: Option<String>) {
        if files.is_empty() {
            return;
        }
        if let Some(d) = self.add.as_mut() {
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
        dialog.prepare(&self.parts);
        self.add = Some(dialog);
    }

    /// Добавить файл на экране и сохранить версию.
    fn add_current(&mut self, ctx: &egui::Context) {
        let (Some(project), Some(d)) = (self.project.clone(), self.add.as_mut()) else {
            return;
        };
        let (Some(file), part) = (d.file().cloned(), d.place(&self.parts)) else {
            return;
        };
        let (Place::Existing(part) | Place::New(part)) = part else {
            d.failed(texts().bad_name.to_string());
            return;
        };
        d.busy = true;
        d.error = None;
        let message = d.message.trim().to_string();
        self.jobs.cancel();
        self.jobs.spawn(ctx, move |progress| {
            progress.step(texts().importing);
            Done::Added(project.add_model(&file, &part, &message))
        });
    }

    /// Файл добавился: следующий или конец.
    pub(super) fn added(&mut self, ctx: &egui::Context, added: AddedModel) {
        // Деталь уже есть для следующего файла, хотя список ещё не перечитан.
        let version = added.version.map(|v| format!("v{v}"));
        match self.parts.iter_mut().find(|p| p.name == added.part) {
            Some(p) => p.latest = version.or(p.latest.take()),
            None => self.parts.push(PartEntry {
                name: added.part.clone(),
                assembly: false,
                latest: version,
                dirty: false,
                model: None,
                stamp: String::new(),
            }),
        }
        let parts = self.parts.clone();
        let Some(d) = self.add.as_mut() else {
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
        let outcome = self.add.take().and_then(|d| d.outcome());
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
