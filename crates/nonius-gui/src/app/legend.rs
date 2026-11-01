//! Что значат цвета и где изменения: легенда и список мест в правой панели.

use eframe::egui::{self, Color32, RichText};
use nonius_scene::Role;

use super::numbers::volume_text;
use crate::texts::texts;
use crate::view::Model;

fn swatch(ui: &mut egui::Ui, role: Role) {
    let [r, g, b] = role.color();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
}

/// Цвета видны всегда, даже когда такого изменения нет: иначе не понять, что
/// значит цвет. С объёмами; галочка прячет роль.
pub fn legend(ui: &mut egui::Ui, model: &Model, look: &mut crate::view::Look) {
    let t = texts();
    for (role, flag) in [
        (Role::Added, &mut look.added),
        (Role::Removed, &mut look.removed),
        (Role::Moved, &mut look.moved),
    ] {
        let volume: f64 = model
            .parts
            .iter()
            .filter(|p| p.role == role)
            .map(|p| p.volume_mm3)
            .sum();
        let present = model.parts.iter().any(|p| p.role == role);
        ui.horizontal(|ui| {
            ui.add_enabled(present, egui::Checkbox::without_text(flag));
            swatch(ui, role);
            if present {
                ui.label(format!("{}  {}", role_name(role), volume_text(volume)));
            } else {
                ui.label(RichText::new(role_name(role)).weak());
            }
        });
    }
    ui.horizontal(|ui| {
        ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
        swatch(ui, Role::After);
        ui.label(RichText::new(t.role_same).weak());
    });
}

/// Сколько мест показывать по одному. Дальше мелочь сводится строкой.
const PLACES_SHOWN: usize = 8;
/// Мелкое место: меньше этой доли от всего изменённого объёма.
const TINY_SHARE: f64 = 0.01;

/// Какие места сводятся строкой «Мелкие места»: только когда их много.
fn tiny_places(volumes: &[f64]) -> Vec<bool> {
    let total: f64 = volumes.iter().sum();
    let tiny: Vec<bool> = volumes.iter().map(|v| *v < total * TINY_SHARE).collect();
    let count = tiny.iter().filter(|t| **t).count();
    if volumes.len() > PLACES_SHOWN && count >= 2 {
        tiny
    } else {
        vec![false; volumes.len()]
    }
}

/// Список мест: ‹ › и щелчок — приблизить. Возвращает кусок для приближения.
pub fn places(ui: &mut egui::Ui, model: &Model, selected: &mut Option<usize>) -> Option<usize> {
    let t = texts();
    let list = model.places();
    if list.is_empty() {
        return None;
    }
    let mut zoom = None;
    let at = selected.and_then(|s| list.iter().position(|&i| i == s));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(t.places).strong());
        let n = list.len();
        if ui.small_button("‹").clicked() {
            zoom = Some(list[at.map_or(n - 1, |a| (a + n - 1) % n)]);
        }
        let shown = at.map_or("–".to_string(), |a| (a + 1).to_string());
        ui.label(format!("{shown} {} {n}", t.place_of));
        if ui.small_button("›").clicked() {
            zoom = Some(list[at.map_or(0, |a| (a + 1) % n)]);
        }
    });
    let volumes: Vec<f64> = list.iter().map(|&i| model.parts[i].volume_mm3).collect();
    let tiny = tiny_places(&volumes);
    let mut row = |ui: &mut egui::Ui, i: usize| {
        let p = &model.parts[i];
        ui.horizontal(|ui| {
            swatch(ui, p.role);
            let text = format!(
                "{} {}   {}",
                role_name(p.role),
                p.number,
                volume_text(p.volume_mm3)
            );
            if ui.selectable_label(*selected == Some(i), text).clicked() {
                zoom = Some(i);
            }
        });
    };
    for (k, &i) in list.iter().enumerate() {
        if !tiny[k] {
            row(ui, i);
        }
    }
    let small: Vec<usize> = (0..list.len()).filter(|&k| tiny[k]).collect();
    if !small.is_empty() {
        let volume: f64 = small.iter().map(|&k| volumes[k]).sum();
        let title = format!(
            "{}: {}, {}",
            t.tiny_places,
            small.len(),
            volume_text(volume)
        );
        egui::CollapsingHeader::new(title)
            .id_salt("tiny-places")
            .show(ui, |ui| {
                for &k in &small {
                    row(ui, list[k]);
                }
            });
    }
    if zoom.is_some() {
        *selected = zoom;
    }
    zoom
}

pub(super) fn role_name(role: Role) -> &'static str {
    let t = texts();
    match role {
        Role::After => t.role_after,
        Role::Before => t.role_before,
        Role::Added => t.role_added,
        Role::Removed => t.role_removed,
        Role::Moved => t.role_moved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_places_fold_only_when_there_are_many() {
        assert_eq!(tiny_places(&[1000.0, 1.0, 1.0]), [false, false, false]);
        let mut many = vec![1000.0; 7];
        many.extend([1.0, 1.0, 1.0]);
        let folded = tiny_places(&many);
        assert_eq!(folded.iter().filter(|t| **t).count(), 3);
        assert!(!folded[0]);
    }
}
