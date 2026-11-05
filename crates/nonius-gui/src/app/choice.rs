//! Выбор с клавиатуры. В окошке: как откроется — в фокусе главная кнопка;
//! стрелки переводят фокус на соседнюю (это делает egui, когда что-то уже в
//! фокусе). В меню: ↑/↓ по пунктам по кругу. Enter или пробел нажимают то,
//! что в фокусе.

use eframe::egui;

/// Кнопки окошка: в фокусе всегда одна из них, сначала — `main`.
pub(super) fn keep_focus(buttons: &[egui::Response], main: usize) {
    if buttons.iter().any(|b| b.has_focus()) {
        return;
    }
    if let Some(b) = buttons.get(main) {
        b.request_focus();
    }
}

/// Стрелки ↑/↓ при открытом меню: окно забирает их раньше egui (иначе он
/// сам двигает фокус — по два шага за нажатие и за край меню). Отдаёт,
/// на сколько пунктов сдвинуться.
pub(super) fn take_menu_arrows(ctx: &egui::Context, raw: &mut egui::RawInput) -> i32 {
    if !egui::Popup::is_any_open(ctx) {
        return 0;
    }
    let mut step = 0;
    raw.events.retain(|e| match e {
        egui::Event::Key {
            key, pressed: true, ..
        } if matches!(key, egui::Key::ArrowDown | egui::Key::ArrowUp) => {
            step += if *key == egui::Key::ArrowDown { 1 } else { -1 };
            false
        }
        egui::Event::Key {
            key: egui::Key::ArrowDown | egui::Key::ArrowUp,
            ..
        } => false,
        _ => true,
    });
    step
}

/// Сдвинуть фокус по пунктам открытого меню, по кругу: вниз с последнего —
/// на первый. Ничего не в фокусе — вниз на первый, вверх на последний.
/// Enter нажимает пункт в фокусе, Esc закрывает меню (это делает egui).
pub(super) fn step_in_menu(ctx: &egui::Context, step: i32) {
    if step == 0 || !egui::Popup::is_any_open(ctx) {
        return;
    }
    let focused = ctx.memory(|m| m.focused());
    // Пункты меню: всё, что можно нажать, в слоях поверх окна.
    let menus: Vec<(egui::LayerId, Vec<(egui::Rect, egui::Id)>)> = ctx.viewport(|v| {
        v.this_pass
            .widgets
            .layers()
            .filter(|(layer, _)| layer.order == egui::Order::Foreground)
            .map(|(layer, rects)| {
                let items = rects
                    .iter()
                    .filter(|w| w.enabled && w.sense.is_focusable())
                    .map(|w| (w.rect, w.id))
                    .collect();
                (*layer, items)
            })
            .collect()
    });
    let painted: Vec<egui::LayerId> = ctx.memory(|m| m.layer_ids().collect());
    // Меню, где фокус; нет такого — то, что открыто первым (вложенное
    // рисуется поверх него).
    let with_focus = menus
        .iter()
        .find(|(_, items)| items.iter().any(|(_, id)| Some(*id) == focused));
    let first = painted.iter().find_map(|l| {
        menus
            .iter()
            .find(|(layer, items)| layer == l && !items.is_empty())
    });
    let Some((_, items)) = with_focus.or(first) else {
        return;
    };
    // Рамка самого меню тоже ловит мышь: пункт — то, внутри чего нет
    // других пунктов.
    let mut items: Vec<(egui::Rect, egui::Id)> = items
        .iter()
        .filter(|(r, id)| {
            !items
                .iter()
                .any(|(o, other)| other != id && r.contains_rect(*o))
        })
        .copied()
        .collect();
    items.sort_by(|a, b| {
        (a.0.top(), a.0.left())
            .partial_cmp(&(b.0.top(), b.0.left()))
            .unwrap()
    });
    let n = items.len() as i32;
    let at = items.iter().position(|(_, id)| Some(*id) == focused);
    let next = match at {
        Some(i) => (i as i32 + step).rem_euclid(n),
        None if step > 0 => (step - 1).rem_euclid(n),
        None => (n + step).rem_euclid(n),
    };
    let id = items[next as usize].1;
    ctx.memory_mut(|m| m.request_focus(id));
    ctx.request_repaint();
}
