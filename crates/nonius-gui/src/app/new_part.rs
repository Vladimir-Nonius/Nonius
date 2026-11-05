//! Окошко «Новая деталь»: название и галочка «Это сборка».

use eframe::egui::{self, RichText};

use super::App;
use crate::texts::texts;

/// Открытое окошко.
#[derive(Debug, Default)]
pub struct NewPart {
    name: String,
    assembly: bool,
    busy: bool,
    error: Option<String>,
    focused: bool,
    /// Папка, куда ляжет деталь: путь от корня, пустой — корень.
    folder: String,
}

impl NewPart {
    /// Окошко для новой детали в папке.
    pub fn in_folder(folder: &str) -> Self {
        Self {
            folder: folder.to_string(),
            ..Self::default()
        }
    }

    /// Создать не вышло: окошко остаётся, причина видна в нём.
    pub fn failed(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
        // Имя исправляют сразу: курсор снова в поле.
        self.focused = false;
    }
}

/// Нарисовать окошко, если оно открыто.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let Some(dialog) = app.tab.new_part.as_mut() else {
        return;
    };
    let mut create = false;
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("new-part")).show(ctx, |ui| {
        ui.set_width(340.0);
        ui.heading(t.new_part_title);
        ui.add_space(6.0);
        ui.label(t.part_name);
        let edit =
            ui.add(egui::TextEdit::singleline(&mut dialog.name).desired_width(f32::INFINITY));
        if !dialog.focused {
            edit.request_focus();
            dialog.focused = true;
        }
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.checkbox(&mut dialog.assembly, t.is_assembly);
        ui.label(RichText::new(super::new_folder::place_words(&dialog.folder)).weak());
        if let Some(e) = &dialog.error {
            ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
        }
        ui.add_space(6.0);
        let ready = !dialog.busy && !dialog.name.trim().is_empty();
        ui.horizontal(|ui| {
            let button = ui.add_enabled(ready, egui::Button::new(t.create));
            create = ready && (button.clicked() || enter);
            close = ui.button(t.cancel).clicked();
        });
    });
    if close || (modal.should_close() && !dialog.busy) {
        app.tab.new_part = None;
    } else if create {
        dialog.busy = true;
        dialog.error = None;
        let (name, assembly) = (dialog.name.trim().to_string(), dialog.assembly);
        let folder = dialog.folder.clone();
        app.create_part(ctx, name, assembly, folder);
    }
}
