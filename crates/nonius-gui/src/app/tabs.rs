//! Вкладки под 3D-видом: «Файлы» и «Числа». Щелчок по открытой вкладке
//! сворачивает панель, тогда 3D занимает всё место.

use eframe::egui::{self, RichText};
use nonius_core::compare::Change;

use super::numbers::{mass_text, tidy};
use super::{pick_label, App};
use crate::source::MassEntry;
use crate::texts::texts;

/// Высота открытой панели: меньше не даём, иначе она сжимается по содержимому,
/// пока оно ещё читается в фоне, и кнопки прыгают под курсором.
const TAB_HEIGHT: f32 = 240.0;

/// Открытая вкладка.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Файлы версии.
    Files,
    /// Было, стало и разница.
    Numbers,
}

/// Строка таблицы «Числа».
struct Row {
    name: String,
    before: String,
    after: String,
    delta: String,
}

/// Нарисовать панель вкладок.
pub fn show(app: &mut App, ctx: &egui::Context) {
    if app.part.is_none() {
        return;
    }
    let t = texts();
    let open = app.tab.is_some();
    // Свёрнутая панель — другая: иначе она вспомнит высоту развёрнутой.
    let id = if open { "tabs" } else { "tabs-closed" };
    egui::TopBottomPanel::bottom(id)
        .resizable(open)
        .default_height(TAB_HEIGHT)
        .height_range(if open {
            TAB_HEIGHT..=f32::INFINITY
        } else {
            0.0..=f32::INFINITY
        })
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for (tab, label) in [(Tab::Files, t.tab_files), (Tab::Numbers, t.tab_numbers)] {
                    if ui.selectable_label(app.tab == Some(tab), label).clicked() {
                        app.tab = (app.tab != Some(tab)).then_some(tab);
                    }
                }
            });
            match app.tab {
                Some(Tab::Files) => {
                    ui.separator();
                    super::files::show(app, ui);
                }
                Some(Tab::Numbers) => {
                    ui.separator();
                    numbers(app, ui);
                }
                None => {}
            }
        });
}

fn numbers(app: &App, ui: &mut egui::Ui) {
    let t = texts();
    let rows = rows(app);
    if rows.is_empty() {
        ui.label(RichText::new(t.no_numbers).weak());
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("numbers-tab")
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Grid::new("numbers-table")
                .num_columns(4)
                .striped(true)
                .min_col_width(110.0)
                .show(ui, |ui| {
                    let head = |ui: &mut egui::Ui, s: &str| ui.label(RichText::new(s).strong());
                    head(ui, t.parameter);
                    head(ui, &format!("{} ({})", t.before, pick_label(app.picks.0)));
                    head(ui, &format!("{} ({})", t.after, pick_label(app.picks.1)));
                    head(ui, t.difference);
                    ui.end_row();
                    for r in rows {
                        ui.label(r.name);
                        ui.label(r.before);
                        ui.label(r.after);
                        ui.label(r.delta);
                        ui.end_row();
                    }
                });
        });
}

/// Масса и материал видны всегда, остальное — только изменившееся.
fn rows(app: &App) -> Vec<Row> {
    let t = texts();
    let mut rows = Vec::new();
    if let Some((_, [before, after, _])) = app.masses.as_ref().filter(|m| m.0 == app.picks) {
        let material = |m: &MassEntry| m.material.clone().unwrap_or_else(|| "—".to_string());
        rows.push(Row {
            name: t.material.to_string(),
            before: material(before),
            after: material(after),
            delta: String::new(),
        });
        let mass = |m: &MassEntry| m.mass_kg.map_or("—".to_string(), mass_text);
        let delta = match (before.mass_kg, after.mass_kg) {
            (Some(b), Some(a)) if a != b => {
                format!(
                    "{}{}",
                    if a > b { "+" } else { "−" },
                    mass_text((a - b).abs())
                )
            }
            _ => String::new(),
        };
        rows.push(Row {
            name: t.mass.to_string(),
            before: mass(before),
            after: mass(after),
            delta,
        });
    }
    if let Some(c) = &app.comparison {
        rows.extend(c.reports.iter().flat_map(|r| &r.changes).map(change_row));
    }
    rows
}

fn change_row(c: &Change) -> Row {
    let side = |v: &Option<String>| v.as_deref().map_or("—".to_string(), tidy);
    Row {
        name: c.subject.clone(),
        before: side(&c.before),
        after: side(&c.after),
        delta: delta(c.before.as_deref(), c.after.as_deref()),
    }
}

/// Разница двух чисел со знаком; не числа — пусто.
fn delta(before: Option<&str>, after: Option<&str>) -> String {
    let number = |s: Option<&str>| s?.trim().replace(',', ".").parse::<f64>().ok();
    match (number(before), number(after)) {
        (Some(b), Some(a)) if a != b => {
            let sign = if a > b { "+" } else { "−" };
            format!("{sign}{}", tidy(&format!("{:.6}", (a - b).abs())))
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differences() {
        assert_eq!(delta(Some("12.25"), Some("13.5")), "+1,25");
        assert_eq!(delta(Some("13.5"), Some("12.25")), "−1,25");
        assert_eq!(delta(Some("1"), Some("1")), "");
        assert_eq!(delta(Some("Ст3"), Some("АМг6")), "");
        assert_eq!(delta(None, Some("5")), "");
    }
}
