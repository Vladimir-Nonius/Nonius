//! Окошко «Сохранить версию»: что изменилось с прошлой версии и обязательное
//! описание «Что изменили и почему».

use eframe::egui::{self, Color32, RichText};
use nonius_scene::Role;

use super::numbers::{mass_text, volume_text};
use super::App;
use crate::source::Pick;
use crate::texts::texts;

/// Открытое окошко.
pub struct SaveDialog {
    /// Деталь.
    pub part: String,
    /// Номер будущей версии.
    pub next: u32,
    /// Изменённые файлы; `None` — ещё читаются.
    pub files: Option<Vec<String>>,
    /// Описание.
    pub message: String,
    /// Версия сохраняется.
    pub saving: bool,
    /// Что пошло не так.
    pub error: Option<String>,
    /// Курсор уже поставлен в поле описания.
    focused: bool,
}

impl SaveDialog {
    /// Пустое окошко для детали.
    pub fn new(part: &str, next: u32) -> Self {
        Self {
            part: part.to_string(),
            next,
            files: None,
            message: String::new(),
            saving: false,
            error: None,
            focused: false,
        }
    }

    /// Можно ли нажать «Сохранить».
    pub fn ready(&self) -> bool {
        !self.saving && !self.message.trim().is_empty()
    }
}

/// Нарисовать окошко, если оно открыто.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let Some(dialog) = app.save.as_ref() else {
        return;
    };
    let (next, part) = (dialog.next, dialog.part.clone());
    let summary = summary(app, &part);
    let mut save = false;
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("save-version")).show(ctx, |ui| {
        let Some(d) = app.save.as_mut() else {
            return;
        };
        ui.set_width(520.0);
        ui.heading(format!("{} «{part}»", t.save_title));
        ui.add_space(6.0);
        egui::Grid::new("save-grid").num_columns(2).show(ui, |ui| {
            ui.label(RichText::new(t.will_be).weak());
            ui.label(RichText::new(format!("v{next}")).strong());
            ui.end_row();
            ui.label(RichText::new(t.changed_files).weak());
            match &d.files {
                None => ui.spinner(),
                Some(f) => ui.add(egui::Label::new(own_files(f).join(", ")).wrap()),
            };
            ui.end_row();
        });
        if let Some(lines) = &summary {
            ui.add_space(6.0);
            egui::Frame::NONE
                .fill(Color32::from_rgb(243, 245, 248))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (text, color) in lines {
                        let mut rich = RichText::new(text);
                        if let Some(c) = color {
                            rich = rich.color(*c);
                        }
                        ui.label(rich);
                    }
                });
        }
        ui.add_space(8.0);
        ui.label(format!("{} *", t.what_why));
        let edit = ui.add(
            egui::TextEdit::multiline(&mut d.message)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        if !d.focused {
            edit.request_focus();
            d.focused = true;
        }
        ui.label(RichText::new(t.save_hint).weak());
        if let Some(e) = &d.error {
            ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
        }
        ui.add_space(6.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if d.saving {
                format!("{}…", t.saving)
            } else {
                format!("{} v{next}", t.save_as)
            };
            if ui
                .add_enabled(d.ready(), egui::Button::new(label))
                .clicked()
            {
                save = true;
            }
            if ui
                .add_enabled(!d.saving, egui::Button::new(t.cancel))
                .clicked()
            {
                close = true;
            }
        });
    });
    // Esc или щелчок мимо — закрыть, пока версия не сохраняется.
    let saving = app.save.as_ref().is_some_and(|d| d.saving);
    if (close || modal.should_close()) && !saving {
        app.save = None;
    } else if save {
        app.save_version(ctx);
    }
}

/// Изменённые файлы без служебных; если других нет — как есть.
fn own_files(files: &[String]) -> Vec<&str> {
    let service = |f: &str| f.ends_with("meta.yaml") || f.ends_with("preview.png");
    let own: Vec<&str> = files
        .iter()
        .map(String::as_str)
        .filter(|f| !service(f))
        .collect();
    if own.is_empty() {
        files.iter().map(String::as_str).collect()
    } else {
        own
    }
}

/// Что изменилось с последней версией — если на экране как раз это сравнение.
fn summary(app: &App, part: &str) -> Option<Vec<(String, Option<Color32>)>> {
    let t = texts();
    let latest = app.versions.first()?.number;
    let picks = (Pick::Version(latest), Pick::Current);
    if app.shown.as_ref() != Some(&(part.to_string(), picks)) {
        return None;
    }
    let mut lines = vec![(format!("{} v{latest}:", t.compared_with), None)];
    if let Some(model) = app.viewport.model() {
        for (role, sign) in [(Role::Added, "+"), (Role::Removed, "−")] {
            let volume: f64 = model
                .parts
                .iter()
                .filter(|p| p.role == role)
                .map(|p| p.volume_mm3)
                .sum();
            if volume > 0.0 {
                let [r, g, b] = role.color();
                let name = super::legend::role_name(role).to_lowercase();
                lines.push((
                    format!("{sign} {name} {}", volume_text(volume)),
                    Some(Color32::from_rgb(r, g, b)),
                ));
            }
        }
    }
    if let Some((p, [before, after, _])) = &app.masses {
        if *p == picks {
            if let (Some(a), Some(b)) = (before.mass_kg, after.mass_kg) {
                lines.push((
                    format!("{} {} → {}", t.mass, mass_text(a), mass_text(b)),
                    None,
                ));
            }
        }
    }
    (lines.len() > 1).then_some(lines)
}
