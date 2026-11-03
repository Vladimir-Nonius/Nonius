//! Карта версий снизу: кружки на линии слева направо, старые первыми, справа
//! «Сейчас». Убранных с карты версий здесь нет. Щелчок и правая кнопка — как
//! в истории.

use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke};

use super::history::{self, Act};
use super::App;
use crate::source::{Pick, VersionEntry};
use crate::texts::texts;

/// Шаг между кружками.
const STEP: f32 = 96.0;
/// Высота рисунка: кружок и две подписи.
const HEIGHT: f32 = 58.0;
/// Радиус кружка.
const RADIUS: f32 = 8.0;
/// Несохранённое — тем же цветом, что в списке деталей.
const NOW: Color32 = Color32::from_rgb(200, 120, 0);
const PLAIN: Color32 = Color32::from_rgb(150, 155, 165);

/// Нарисовать карту выбранной детали.
pub fn show(app: &mut App, ctx: &egui::Context) {
    // Карта нужна, когда выбирать есть из чего.
    let count = history::on_map(&app.versions).len() + usize::from(app.part_dirty());
    if app.part.is_none() || count < 2 {
        return;
    }
    let mut act = None;
    egui::TopBottomPanel::bottom("map")
        .resizable(true)
        .default_height(HEIGHT + 10.0)
        .height_range(HEIGHT + 8.0..=220.0)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            egui::ScrollArea::horizontal()
                .id_salt("map")
                .stick_to_right(true)
                .auto_shrink([false, true])
                .show(ui, |ui| nodes(app, ui, &mut act));
        });
    app.apply(ctx, act);
}

/// Линия и кружки.
fn nodes(app: &App, ui: &mut egui::Ui, act: &mut Option<Act>) {
    let t = texts();
    let dirty = app.part_dirty();
    let versions = history::on_map(&app.versions);
    let count = versions.len() + usize::from(dirty);
    let latest = app.versions.first().map(|v| v.number);
    let size = egui::vec2(count as f32 * STEP, HEIGHT);
    let (area, _) = ui.allocate_exact_size(size, Sense::hover());
    let center = |k: usize| area.left_top() + egui::vec2((k as f32 + 0.5) * STEP, RADIUS + 3.0);
    let line = Stroke::new(2.0_f32, PLAIN);
    ui.painter()
        .line_segment([center(0), center(count - 1)], line);

    let picks = app.picks;
    let part = app.part.as_deref().unwrap_or_default();
    for (k, i) in (0..versions.len()).rev().enumerate() {
        let v = &versions[i];
        let me = Pick::Version(v.number);
        let r = node(ui, center(k), me, picks, &version_text(v), date(&v.date));
        let r = r.on_hover_text(version_hint(v));
        if r.clicked() {
            *act = Some(Act::Show(history::version_pair(&versions, i)));
        }
        history::version_menu(&r, part, me, Some(picks), latest, false, act);
    }
    if dirty {
        let at = center(count - 1);
        let r = node(ui, at, Pick::Current, picks, t.now, "");
        let r = r.on_hover_text(t.not_saved);
        if let (true, Some(p)) = (r.clicked(), history::now_pair(&app.versions)) {
            *act = Some(Act::Show(p));
        }
        if !app.versions.is_empty() {
            history::now_menu(&r, picks, act);
        }
    }
}

/// Кружок с подписями. Для тестов и экранного диктора — кнопка «Карта v2».
fn node(
    ui: &mut egui::Ui,
    at: egui::Pos2,
    me: Pick,
    picks: (Pick, Pick),
    title: &str,
    sub: &str,
) -> egui::Response {
    let t = texts();
    let rect = egui::Rect::from_min_size(
        at - egui::vec2(STEP / 2.0 - 2.0, RADIUS + 3.0),
        egui::vec2(STEP - 4.0, HEIGHT),
    );
    let r = ui
        .interact(rect, ui.id().with(("map-node", me)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let label = format!("{} {title}", t.map_node);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));

    let mark = history::mark(me, picks);
    let fill = match (me, mark) {
        (Pick::Current, _) => NOW,
        (_, Some(m)) => m.color,
        _ => PLAIN,
    };
    let painter = ui.painter();
    painter.circle_filled(at, RADIUS, fill);
    if let Some(m) = mark {
        painter.circle_stroke(at, RADIUS + 3.0, Stroke::new(2.0_f32, m.color));
    }
    let color = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let below = at + egui::vec2(0.0, RADIUS + 5.0);
    painter.text(
        below,
        Align2::CENTER_TOP,
        title,
        FontId::proportional(13.0),
        color,
    );
    let lower = below + egui::vec2(0.0, 16.0);
    painter.text(
        lower,
        Align2::CENTER_TOP,
        sub,
        FontId::proportional(11.0),
        weak,
    );
    r
}

fn version_text(v: &VersionEntry) -> String {
    format!("v{}", v.number)
}

/// Дата без времени.
fn date(text: &str) -> &str {
    text.split(' ').next().unwrap_or(text)
}

/// Подсказка: номер, дата, автор, описание.
fn version_hint(v: &VersionEntry) -> String {
    let mut text = format!("{} · {} · {}", version_text(v), v.date, v.author);
    if !v.summary.is_empty() {
        text = format!("{text}\n{}", v.summary);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_drops_the_time() {
        assert_eq!(date("2026-10-27 12:00"), "2026-10-27");
        assert_eq!(date(""), "");
    }
}
