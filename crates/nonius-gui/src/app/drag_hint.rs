//! Что видно, пока что-то несут мышью. Деталь из дерева или «Структуры» —
//! её имя летит за курсором, строка на старом месте бледнеет. Файл из
//! Проводника — то же у курсора. Подпись простыми словами: «Переместить в
//! «Стена»», «Скопировать в «Стена»» (оригинал остаётся), «Добавить в
//! деталь …», «Сюда нельзя». Место заливается цветом, свёрнутая папка
//! раскрывается, если подержать над ней курсор.

use eframe::egui;

use super::drop_zones::Zone;
use super::tree_folders::{self, Dragged};
use super::App;
use crate::texts::texts;

/// Сколько держать курсор над свёрнутой папкой, чтобы она раскрылась, с.
const UNFOLD_AFTER: f64 = 0.6;

/// Что будет с тем, что несут.
#[derive(Clone, Copy)]
enum Carry {
    /// Деталь проекта переезжает.
    Move,
    /// Файл из Проводника копируется: оригинал остаётся на месте.
    Copy,
}

impl App {
    /// Файлы из Проводника несут над окном: подсветить и подписать, куда они
    /// лягут. Цель запоминается: кадр после броска может прийти позже,
    /// когда мышь уже ушла.
    pub(super) fn show_drop_target(&mut self, ctx: &egui::Context) {
        self.raise_while_carried(ctx);
        if ctx.input(|i| i.raw.hovered_files.is_empty()) {
            if ctx.input(|i| i.raw.dropped_files.is_empty()) {
                self.tab.drop_aim = None;
            }
            return;
        }
        // Курсор системы двигается без событий окна: смотреть каждый кадр.
        ctx.request_repaint();
        let Some(pos) = self.pointer_in_window(ctx) else {
            return;
        };
        let found = self.zone_at(pos);
        let text = match &found {
            Some((rect, zone)) => {
                highlight(ctx, *rect);
                self.unfold_on_hover(ctx, zone);
                words(Carry::Copy, zone)
            }
            None => texts().drop_nowhere.to_string(),
        };
        cursor_label(ctx, pos, &text, found.is_none());
        self.tab.drop_aim = found.map(|(_, zone)| zone);
    }

    /// Деталь тащат по дереву или «Структуре»: её имя летит за курсором,
    /// под ним — куда она переедет.
    pub(super) fn show_drag_ghost(&mut self, ctx: &egui::Context) {
        let Some(dragged) = egui::DragAndDrop::payload::<Dragged>(ctx) else {
            return;
        };
        let Some(pos) = ctx.pointer_latest_pos() else {
            return;
        };
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        let t = texts();
        let found = self.zone_at(pos);
        let line = match &found {
            Some((rect, zone)) => {
                highlight(ctx, *rect);
                self.unfold_on_hover(ctx, zone);
                // Брошенная на деталь ложится рядом с ней, в её папку.
                let folder = match zone {
                    Zone::Folder(f) => f.clone(),
                    Zone::Part(p) => self.folder_of(p),
                };
                if folder == self.folder_of(&dragged.0) {
                    t.same_place.to_string()
                } else {
                    words(Carry::Move, &Zone::Folder(folder))
                }
            }
            None => t.drop_nowhere.to_string(),
        };
        cursor_label(ctx, pos, &format!("{}\n{line}", dragged.0), found.is_none());
    }

    /// Файл из Проводника коснулся окна — поднять его поверх всех, чтобы
    /// Проводник не закрывал папки; отпустили или унесли — вернуть как было.
    fn raise_while_carried(&mut self, ctx: &egui::Context) {
        let carried = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if carried == self.raised {
            return;
        }
        self.raised = carried;
        let level = if carried {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
        if !carried {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    /// Курсор держат над свёрнутой папкой — раскрыть её, как в Проводнике.
    fn unfold_on_hover(&mut self, ctx: &egui::Context, zone: &Zone) {
        let folded = match zone {
            Zone::Folder(f) if self.tab.folded.contains(f) => f,
            _ => {
                self.tab.hover_since = None;
                return;
            }
        };
        let now = ctx.input(|i| i.time);
        match &self.tab.hover_since {
            Some((f, since)) if f == folded => {
                if now - since >= UNFOLD_AFTER {
                    self.tab.folded.remove(folded);
                    self.tab.hover_since = None;
                }
            }
            _ => self.tab.hover_since = Some((folded.clone(), now)),
        }
        ctx.request_repaint();
    }
}

/// «Переместить в «Стена»», «Скопировать в корень проекта», «Добавить в
/// деталь «RAIL-01»».
fn words(carry: Carry, zone: &Zone) -> String {
    let t = texts();
    let verb = match carry {
        Carry::Move => t.carry_move,
        Carry::Copy => t.carry_copy,
    };
    match zone {
        Zone::Folder(f) if f.is_empty() => format!("{verb} {}", t.to_root),
        Zone::Folder(f) => format!("{verb} в «{}»", tree_folders::last(f)),
        Zone::Part(p) => format!("{} «{p}»", t.add_to_part),
    }
}

/// Залить место, куда ляжет брошенное.
fn highlight(ctx: &egui::Context, rect: egui::Rect) {
    let selection = ctx.style().visuals.selection;
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("drop-target"),
    ));
    painter.rect_filled(rect, 3.0, selection.bg_fill.gamma_multiply(0.35));
    painter.rect_stroke(rect, 3.0, selection.stroke, egui::StrokeKind::Outside);
}

/// Табличка у курсора: что несут и куда. Некуда — красным.
fn cursor_label(ctx: &egui::Context, pos: egui::Pos2, text: &str, bad: bool) {
    egui::Area::new(egui::Id::new("cursor-label"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + egui::vec2(18.0, 14.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let mut text = egui::RichText::new(text);
                if bad {
                    text = text.color(ui.visuals().error_fg_color);
                }
                // Без переноса: табличка узкая, слова рвались бы по слогам.
                ui.add(egui::Label::new(text).extend());
            });
        });
}
