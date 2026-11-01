//! Дерево слева: поиск, детали проекта, внутри выбранной — её история.

use eframe::egui::{self, Color32, RichText};

use super::open_menu::{self, Target};
use super::App;
use crate::source::{PartEntry, Pick};
use crate::texts::texts;

/// Паспорт детали: он есть всегда, даже без файла модели.
const META_FILE: &str = nonius_core::project::META_FILE;

/// Нарисовать дерево.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = texts();
    let mut clicked = None;
    let mut asked = None;
    egui::SidePanel::left("parts")
        .default_width(250.0)
        .show(ctx, |ui| {
            if let Some(p) = &app.project {
                ui.add_space(6.0);
                ui.heading(p.title());
            }
            ui.label(RichText::new(t.parts).strong());
            let search = egui::TextEdit::singleline(&mut app.search)
                .hint_text(t.search_parts)
                .desired_width(f32::INFINITY);
            ui.add(search);
            ui.separator();
            if app.parts.is_empty() && !app.jobs.busy() {
                ui.label(RichText::new(t.no_parts).weak());
            }
            let found: Vec<PartEntry> = app
                .parts
                .iter()
                .filter(|p| matches(&p.name, &app.search))
                .cloned()
                .collect();
            if found.is_empty() && !app.parts.is_empty() {
                ui.label(RichText::new(t.nothing_found).weak());
            }
            egui::ScrollArea::vertical()
                .id_salt("tree")
                .auto_shrink(false)
                .show(ui, |ui| {
                    for p in &found {
                        let selected = app.part.as_ref() == Some(&p.name);
                        let open = selected && app.tree_open;
                        ui.horizontal(|ui| {
                            let arrow = if open { "▾" } else { "▸" };
                            let toggle = ui.add(egui::Button::new(arrow).frame(false)).clicked();
                            let r = part_row(ui, p, selected);
                            if toggle && selected {
                                app.tree_open = !open;
                            } else if toggle || r.clicked() {
                                app.tree_open = true;
                                if !selected {
                                    clicked = Some(p.name.clone());
                                }
                            }
                            open_menu::attach(&r, &app.programs, || target(p), &mut asked);
                        });
                        if open {
                            ui.indent("history", |ui| super::history::show(app, ui));
                        }
                    }
                });
        });
    if let Some(name) = clicked {
        app.select_part(ctx, &name);
    }
    app.answer_menu(ctx, asked);
}

/// Подходит ли деталь под поиск: без учёта регистра.
fn matches(name: &str, search: &str) -> bool {
    name.to_lowercase().contains(&search.trim().to_lowercase())
}

/// Строка детали: имя, последняя версия, сборка; несохранённое — рыжим.
fn part_row(ui: &mut egui::Ui, p: &PartEntry, selected: bool) -> egui::Response {
    let t = texts();
    let mut text = p.name.clone();
    if let Some(v) = &p.latest {
        text = format!("{text}   {v}");
    }
    if p.assembly {
        text = format!("{text}  ({})", t.assembly);
    }
    let mut rich = RichText::new(text);
    if p.dirty {
        rich = rich.color(Color32::from_rgb(200, 120, 0));
    }
    let r = ui.selectable_label(selected, rich);
    if p.dirty {
        r.on_hover_text(t.unsaved_changes)
    } else {
        r
    }
}

fn target(p: &PartEntry) -> Target {
    Target {
        part: p.name.clone(),
        pick: Pick::Current,
        // Папку детали можно показать и без файла модели.
        path: p.model.clone().unwrap_or_else(|| META_FILE.to_string()),
        openable: p.model.is_some(),
        model_slot: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_ignores_case_and_spaces() {
        assert!(matches("Кронштейн", " крон "));
        assert!(matches("кронштейн", ""));
        assert!(!matches("корпус", "крон"));
    }
}
