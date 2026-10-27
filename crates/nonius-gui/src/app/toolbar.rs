//! Полоса над 3D-видом: какие версии сравнить, режим показа, виды.

use eframe::egui::{self, RichText};

use super::{pick_label, picture_name, App};
use crate::source::Pick;
use crate::texts::texts;
use crate::view::{Mode, StdView};

/// Нарисовать полосу.
pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let t = texts();
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .fill(ui.visuals().panel_fill)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                if app.part.is_some() {
                    versions(app, ui);
                    ui.separator();
                    let changes = app.viewport.model().is_some_and(|m| m.has_changes);
                    ui.add_enabled_ui(changes, |ui| {
                        for (mode, label) in [
                            (Mode::Before, t.mode_before),
                            (Mode::Changes, t.mode_changes),
                            (Mode::After, t.mode_after),
                        ] {
                            ui.selectable_value(&mut app.mode, mode, label);
                        }
                    });
                    ui.separator();
                }
                let ctx = ui.ctx().clone();
                for (view, label, key) in [
                    (StdView::Iso, t.view_iso, 7),
                    (StdView::Front, t.view_front, 1),
                    (StdView::Top, t.view_top, 5),
                    (StdView::Right, t.view_right, 4),
                ] {
                    let hint = format!("Ctrl+{key}");
                    if ui.button(label).on_hover_text(hint).clicked() {
                        app.viewport.set_view(&ctx, Some(view));
                    }
                }
                ui.menu_button("…", |ui| {
                    for (view, label, key) in [
                        (StdView::Back, t.view_back, 2),
                        (StdView::Left, t.view_left, 3),
                        (StdView::Bottom, t.view_bottom, 6),
                    ] {
                        if ui.button(format!("{label}   Ctrl+{key}")).clicked() {
                            app.viewport.set_view(&ctx, Some(view));
                            ui.close();
                        }
                    }
                });
                if ui.button(t.fit).clicked() {
                    app.viewport.set_view(&ctx, None);
                }
            });
        });
}

/// Выбор «Было» и «Стало».
fn versions(app: &mut App, ui: &mut egui::Ui) {
    let t = texts();
    if app.versions.is_empty() {
        ui.label(RichText::new(t.no_versions).weak());
        return;
    }
    let dirty = app
        .parts
        .iter()
        .any(|p| Some(&p.name) == app.part.as_ref() && p.dirty);
    let mut choices: Vec<(Pick, String)> = Vec::new();
    if dirty {
        choices.push((Pick::Current, t.current.to_string()));
    }
    for v in &app.versions {
        let mut text = format!("v{} · {} · {}", v.number, v.date, v.author);
        if !v.summary.is_empty() {
            text = format!("{text} — {}", v.summary);
        }
        choices.push((Pick::Version(v.number), text));
    }
    let old = app.picks;
    for (label, pick, id) in [
        (t.before, &mut app.picks.0, "before"),
        (t.after, &mut app.picks.1, "after"),
    ] {
        ui.label(label);
        egui::ComboBox::from_id_salt(id)
            .selected_text(pick_label(*pick))
            .width(70.0)
            .show_ui(ui, |ui| {
                for (p, text) in &choices {
                    ui.selectable_value(pick, *p, text);
                }
            });
    }
    if app.picks != old {
        let ctx = ui.ctx().clone();
        app.compare(&ctx);
    }
}

/// Сохранить то, что сейчас видно в 3D, в PNG.
pub fn save_picture(app: &mut App) {
    let t = texts();
    let Some(part) = app.part.clone() else {
        return;
    };
    let name = picture_name(&part, app.picks);
    let Some(path) = choose_path(&name) else {
        return;
    };
    let result = app
        .viewport
        .image()
        .and_then(|img| img.ok_or_else(|| anyhow::anyhow!("{}", t.no_scene)))
        .and_then(|img| img.save_png(&path));
    app.message = Some(match result {
        Ok(()) => format!("{}: {}", t.picture_saved, path.display()),
        Err(e) => format!("{}: {e:#}", t.error),
    });
}

#[cfg(windows)]
fn choose_path(name: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .set_file_name(name)
        .add_filter("PNG", &["png"])
        .save_file()
}

/// Без системного окна выбора — в текущую папку.
#[cfg(not(windows))]
fn choose_path(name: &str) -> Option<std::path::PathBuf> {
    std::env::current_dir().ok().map(|d| d.join(name))
}
