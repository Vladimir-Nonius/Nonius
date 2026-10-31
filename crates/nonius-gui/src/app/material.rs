//! «Материал и масса» в правой панели: материал детали сейчас (из
//! справочника или свой) и масса «было → стало». Без материала массы нет.

use eframe::egui::{self, Color32, RichText};

use super::numbers::{density_text, mass_text};
use super::{pick_label, App};
use crate::source::{MassEntry, MaterialEntry};
use crate::texts::texts;

/// Ввод своего материала: название и плотность.
#[derive(Debug, Default)]
pub struct Custom {
    /// Поля ввода открыты.
    pub open: bool,
    name: String,
    density: String,
    error: Option<String>,
}

/// Что выбрали в списке.
enum Choice {
    Catalog(String),
    Custom,
    Clear,
}

/// Нарисовать блок.
pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let t = texts();
    if app.part.is_none() || app.versions.is_empty() && !dirty(app) {
        return;
    }
    ui.add_space(8.0);
    ui.label(RichText::new(t.material_mass).strong());
    let Some((picks, [before, after, now])) = app.masses.clone().filter(|m| m.0 == app.picks)
    else {
        ui.spinner();
        return;
    };
    let catalog = catalog_entry(&app.materials, &now);
    let mut choice = None;
    let busy = app.material_busy;
    egui::Grid::new("material").num_columns(2).show(ui, |ui| {
        ui.label(RichText::new(t.material).weak());
        let shown = now
            .material
            .clone()
            .unwrap_or_else(|| t.no_material.to_string());
        ui.add_enabled_ui(!busy, |ui| {
            egui::ComboBox::from_id_salt("material-choice")
                .selected_text(shown)
                .width(150.0)
                .height(400.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(now.material.is_none(), t.no_material)
                        .clicked()
                    {
                        choice = Some(Choice::Clear);
                    }
                    for m in &app.materials {
                        let text = format!("{}   {}", m.name, density_text(m.density_kg_m3));
                        let on = catalog.is_some_and(|c| c.name == m.name);
                        if ui.selectable_label(on, text).clicked() {
                            choice = Some(Choice::Catalog(m.name.clone()));
                        }
                    }
                    ui.separator();
                    let custom = now.material.is_some() && catalog.is_none();
                    if ui.selectable_label(custom, t.custom_material).clicked() {
                        choice = Some(Choice::Custom);
                    }
                });
        });
        ui.end_row();
        if let Some(d) = now.density_kg_m3 {
            ui.label(RichText::new(t.density).weak());
            ui.horizontal(|ui| {
                ui.label(density_text(d));
                if catalog.is_some() {
                    ui.label(RichText::new(t.from_catalog).small().weak());
                }
            });
            ui.end_row();
        }
        ui.label(RichText::new(t.mass).weak());
        mass_line(ui, picks, &before, &after);
        ui.end_row();
    });
    let ctx = ui.ctx().clone();
    match choice {
        Some(Choice::Catalog(name)) => {
            app.custom.open = false;
            app.set_material(&ctx, Some((name, None)));
        }
        Some(Choice::Clear) => {
            app.custom.open = false;
            app.set_material(&ctx, None);
        }
        Some(Choice::Custom) => {
            let c = &mut app.custom;
            c.open = true;
            c.error = None;
            c.name = now.material.clone().unwrap_or_default();
            c.density = now
                .density_kg_m3
                .map(|d| format!("{d}"))
                .unwrap_or_default();
        }
        None => {}
    }
    if app.custom.open {
        custom(app, ui);
    }
    ui.label(RichText::new(t.material_hint).weak());
}

fn dirty(app: &App) -> bool {
    app.parts
        .iter()
        .any(|p| Some(&p.name) == app.part.as_ref() && p.dirty)
}

/// Материал детали — из справочника (то же название и та же плотность)?
fn catalog_entry<'a>(materials: &'a [MaterialEntry], now: &MassEntry) -> Option<&'a MaterialEntry> {
    let name = now.material.as_deref()?;
    materials
        .iter()
        .find(|m| m.name == name && Some(m.density_kg_m3) == now.density_kg_m3)
}

/// «v2: 113 г → текущее: 126 г»; одна версия — просто масса. Нет массы —
/// почему, оранжевым.
fn mass_line(
    ui: &mut egui::Ui,
    picks: (crate::source::Pick, crate::source::Pick),
    before: &MassEntry,
    after: &MassEntry,
) {
    let warn = Color32::from_rgb(170, 100, 0);
    let text = |m: &MassEntry| m.mass_kg.map(mass_text);
    match (text(before), text(after)) {
        (_, None) => {
            let why = after.no_mass.clone().unwrap_or_default();
            ui.add(egui::Label::new(RichText::new(why).color(warn)).wrap());
        }
        (b, Some(a)) if picks.0 == picks.1 || b.as_ref() == Some(&a) => {
            ui.label(RichText::new(a).strong());
        }
        (b, Some(a)) => {
            let b = b.unwrap_or_else(|| "—".into());
            ui.label(format!(
                "{}: {b} → {}: {a}",
                pick_label(picks.0),
                pick_label(picks.1)
            ));
        }
    }
}

/// Поля «свой материал»: название, плотность, «Применить».
fn custom(app: &mut App, ui: &mut egui::Ui) {
    let t = texts();
    let mut apply = false;
    let c = &mut app.custom;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut c.name)
                .hint_text(t.material_name)
                .desired_width(110.0),
        );
        ui.add(
            egui::TextEdit::singleline(&mut c.density)
                .hint_text("7850")
                .desired_width(60.0),
        );
        ui.label(t.kg_m3);
    });
    ui.horizontal(|ui| {
        if ui.button(t.apply).clicked() {
            apply = true;
        }
        if ui.button(t.cancel).clicked() {
            c.open = false;
        }
    });
    if let Some(e) = &c.error {
        ui.label(RichText::new(e.as_str()).color(ui.visuals().error_fg_color));
    }
    if apply {
        match parse_density(&c.density) {
            Some(d) if !c.name.trim().is_empty() => {
                c.error = None;
                let name = c.name.trim().to_string();
                let ctx = ui.ctx().clone();
                app.set_material(&ctx, Some((name, Some(d))));
            }
            _ => c.error = Some(t.bad_density.to_string()),
        }
    }
}

/// «7 850», «7850,5» → число больше нуля.
fn parse_density(text: &str) -> Option<f64> {
    let clean: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if c == ',' { '.' } else { c })
        .collect();
    clean
        .parse::<f64>()
        .ok()
        .filter(|d| d.is_finite() && *d > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_is_read_as_a_mechanic_writes_it() {
        assert_eq!(parse_density("7850"), Some(7850.0));
        assert_eq!(parse_density(" 7 850 "), Some(7850.0));
        assert_eq!(parse_density("2,7e3"), Some(2700.0));
        assert_eq!(parse_density("1150,5"), Some(1150.5));
        assert_eq!(parse_density("0"), None);
        assert_eq!(parse_density("-5"), None);
        assert_eq!(parse_density("сталь"), None);
        assert_eq!(parse_density(""), None);
    }

    #[test]
    fn catalog_needs_same_density() {
        let list = vec![MaterialEntry {
            name: "Ст3".into(),
            density_kg_m3: 7850.0,
        }];
        let mut now = MassEntry {
            material: Some("Ст3".into()),
            density_kg_m3: Some(7850.0),
            ..MassEntry::default()
        };
        assert!(catalog_entry(&list, &now).is_some());
        now.density_kg_m3 = Some(7800.0);
        assert!(catalog_entry(&list, &now).is_none(), "плотность своя");
    }
}
