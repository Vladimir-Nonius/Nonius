//! Рамка мышью, как в Проводнике: зажать кнопку на пустом месте дерева или
//! «Структуры» и тянуть — выделяется всё, что рамка задела; с Ctrl —
//! добавляется к прежнему. Строку или карточку тянуть — по-прежнему
//! переносить деталь: они лежат поверх пустого места.

use eframe::egui;

use super::shown::{self, Shown};
use super::App;

/// Что сделали с пустым местом за кадр.
#[derive(Default)]
pub(super) struct Sweep {
    /// Щёлкнули: снять выделение.
    pub clicked: bool,
    /// Тянут рамку: её место.
    pub area: Option<egui::Rect>,
    /// Отпустили рамку.
    pub done: bool,
}

/// Пустое место вида. Звать первым в содержимом прокрутки: всё, что
/// нарисуется потом, ляжет поверх и заберёт мышь себе.
pub(super) fn background(ui: &mut egui::Ui, id: &str) -> egui::Response {
    ui.interact(
        ui.clip_rect(),
        egui::Id::new(id),
        egui::Sense::click_and_drag(),
    )
}

/// Что сделали с пустым местом; рамку — нарисовать поверх. Звать
/// последним в содержимом прокрутки.
pub(super) fn track(ui: &egui::Ui, bg: &egui::Response) -> Sweep {
    let mut sweep = Sweep {
        clicked: bg.clicked(),
        done: bg.drag_stopped(),
        area: None,
    };
    if bg.dragged() {
        let (from, to) = ui.input(|i| (i.pointer.press_origin(), i.pointer.latest_pos()));
        if let (Some(from), Some(to)) = (from, to) {
            let area = egui::Rect::from_two_pos(from, to);
            let color = ui.visuals().selection.bg_fill;
            let stroke = egui::Stroke::new(1.0_f32, color);
            ui.painter().rect(
                area,
                2.0,
                color.gamma_multiply(0.25),
                stroke,
                egui::StrokeKind::Inside,
            );
            sweep.area = Some(area);
        }
    }
    sweep
}

impl App {
    /// Рамка задела предметы из `list`. В дереве (`rows`) строка задета,
    /// если рамка прошла по её высоте: строки короткие, справа пусто.
    pub(super) fn sweep(&mut self, ctx: &egui::Context, sweep: &Sweep, list: &[Shown], rows: bool) {
        if let Some(area) = sweep.area {
            let add = ctx.input(|i| i.modifiers.command);
            let touched = shown::touched(list, area, rows);
            self.tab.selection.sweep(touched, add);
            self.tab.keys_to_files = false;
        }
        if sweep.done {
            self.tab.selection.swept();
        }
    }
}
