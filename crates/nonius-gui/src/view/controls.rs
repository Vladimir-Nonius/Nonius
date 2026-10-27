//! Мышь и клавиши как в SolidWorks → движения камеры.
//!
//! | Действие | Движение |
//! |---|---|
//! | средняя кнопка (или левая) | вращать |
//! | Ctrl + средняя | сдвигать |
//! | Shift + средняя | масштаб |
//! | колесо | масштаб к курсору |
//! | двойной щелчок средней, F | показать целиком |
//! | Ctrl+1…7 | виды: спереди, сзади, слева, справа, сверху, снизу, изометрия |
//! | стрелки | повернуть на 15°, с Shift — на 90° |
//! | Ctrl + стрелки | сдвинуть |
//! | Alt + ← → | повернуть вокруг оси взгляда |

use eframe::egui::{Key, Modifiers, PointerButton};
use glam::Vec2;

use super::camera::StdView;

/// Что произошло с мышью и клавиатурой за кадр.
#[derive(Debug, Clone, Default)]
pub struct Input {
    /// Сдвиг мыши с нажатой кнопкой, пиксели (Y вниз).
    pub drag: Vec2,
    /// Какой кнопкой тянут.
    pub button: Option<PointerButton>,
    /// Ctrl, Shift, Alt.
    pub modifiers: Modifiers,
    /// Прокрутка колеса: плюс — от себя.
    pub scroll: f32,
    /// Масштаб жестом на сенсорной панели (1 — без изменений).
    pub pinch: f32,
    /// Курсор: от −1 до 1, Y вверх.
    pub cursor: Option<Vec2>,
    /// Двойной щелчок средней кнопкой.
    pub double_middle: bool,
    /// Нажатые клавиши.
    pub keys: Vec<(Key, Modifiers)>,
}

/// Движение камеры.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Повернуть на углы (радианы) вокруг вертикали и горизонтали экрана.
    Rotate(Vec2),
    /// Сдвинуть на пиксели.
    Pan(Vec2),
    /// Приблизить в `factor` раз к точке `at`.
    Zoom {
        /// Во сколько раз.
        factor: f32,
        /// Точка экрана, которая остаётся на месте.
        at: Vec2,
    },
    /// Повернуть вокруг оси взгляда.
    Roll(f32),
    /// Стандартный вид.
    View(StdView),
    /// Показать целиком.
    Fit,
}

/// Радиан на пиксель при вращении мышью.
const ROTATE_PER_PIXEL: f32 = 0.008;
/// Шаг колеса: во сколько раз за одну «ступеньку» (≈50 единиц прокрутки).
const WHEEL_STEP: f32 = 1.2;
/// Пикселей сдвига на нажатие Ctrl+стрелки.
const PAN_STEP: f32 = 40.0;

/// Перевести ввод в движения камеры. `wheel_reverse` — колесо наоборот.
pub fn actions(input: &Input, wheel_reverse: bool) -> Vec<Action> {
    let mut out = Vec::new();
    let m = input.modifiers;
    let at = input.cursor.unwrap_or(Vec2::ZERO);

    if input.button.is_some() && input.drag != Vec2::ZERO {
        out.push(if m.command || m.ctrl {
            Action::Pan(input.drag)
        } else if m.shift {
            // Вниз — приблизить, вверх — отдалить; масштаб к центру вида.
            Action::Zoom {
                factor: (input.drag.y * 0.01).exp(),
                at: Vec2::ZERO,
            }
        } else {
            Action::Rotate(input.drag * ROTATE_PER_PIXEL)
        });
    }

    // Как в SolidWorks: колесо от себя — отдалить.
    let sign = if wheel_reverse { 1.0 } else { -1.0 };
    let mut factor = WHEEL_STEP.powf(sign * input.scroll / 50.0);
    if input.pinch > 0.0 {
        factor *= input.pinch;
    }
    if (factor - 1.0).abs() > 1e-4 {
        out.push(Action::Zoom { factor, at });
    }

    if input.double_middle {
        out.push(Action::Fit);
    }

    let step = |mods: Modifiers| {
        if mods.shift {
            std::f32::consts::FRAC_PI_2
        } else {
            15f32.to_radians()
        }
    };
    for &(key, mods) in &input.keys {
        let ctrl = mods.command || mods.ctrl;
        let dir = match key {
            Key::ArrowLeft => Some(Vec2::new(-1.0, 0.0)),
            Key::ArrowRight => Some(Vec2::new(1.0, 0.0)),
            Key::ArrowUp => Some(Vec2::new(0.0, -1.0)),
            Key::ArrowDown => Some(Vec2::new(0.0, 1.0)),
            _ => None,
        };
        let view = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
        ]
        .iter()
        .position(|&k| k == key)
        .map(|i| StdView::ALL[i]);
        match (dir, view) {
            (Some(d), _) if mods.alt => {
                if d.x != 0.0 {
                    out.push(Action::Roll(d.x * step(mods)));
                }
            }
            (Some(d), _) if ctrl => out.push(Action::Pan(d * PAN_STEP)),
            (Some(d), _) => out.push(Action::Rotate(d * step(mods))),
            (None, Some(v)) if ctrl => out.push(Action::View(v)),
            _ if key == Key::F && !ctrl => out.push(Action::Fit),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drag(button: PointerButton, modifiers: Modifiers) -> Vec<Action> {
        let input = Input {
            drag: Vec2::new(10.0, 0.0),
            button: Some(button),
            modifiers,
            ..Default::default()
        };
        actions(&input, false)
    }

    #[test]
    fn mouse_like_solidworks() {
        use PointerButton::*;
        assert!(matches!(
            drag(Middle, Modifiers::NONE)[..],
            [Action::Rotate(_)]
        ));
        assert!(matches!(
            drag(Primary, Modifiers::NONE)[..],
            [Action::Rotate(_)]
        ));
        assert_eq!(
            drag(Middle, Modifiers::CTRL),
            [Action::Pan(Vec2::new(10.0, 0.0))]
        );
        assert!(matches!(
            drag(Middle, Modifiers::SHIFT)[..],
            [Action::Zoom { .. }]
        ));
    }

    #[test]
    fn wheel_away_zooms_out_unless_reversed() {
        let input = Input {
            scroll: 50.0,
            cursor: Some(Vec2::new(0.5, 0.5)),
            ..Default::default()
        };
        let zoom = |reverse| match actions(&input, reverse)[..] {
            [Action::Zoom { factor, at }] => {
                assert_eq!(at, Vec2::new(0.5, 0.5));
                factor
            }
            ref other => panic!("{other:?}"),
        };
        assert!(zoom(false) < 1.0);
        assert!(zoom(true) > 1.0);
        assert!(actions(&Input::default(), false).is_empty());
    }

    #[test]
    fn keys() {
        let press = |key, modifiers| {
            actions(
                &Input {
                    keys: vec![(key, modifiers)],
                    ..Default::default()
                },
                false,
            )
        };
        assert_eq!(
            press(Key::Num1, Modifiers::CTRL),
            [Action::View(StdView::Front)]
        );
        assert_eq!(
            press(Key::Num7, Modifiers::CTRL),
            [Action::View(StdView::Iso)]
        );
        assert!(press(Key::Num1, Modifiers::NONE).is_empty());
        assert_eq!(press(Key::F, Modifiers::NONE), [Action::Fit]);
        assert_eq!(
            press(Key::ArrowLeft, Modifiers::SHIFT),
            [Action::Rotate(Vec2::new(-std::f32::consts::FRAC_PI_2, 0.0))]
        );
        assert!(matches!(
            press(Key::ArrowRight, Modifiers::ALT)[..],
            [Action::Roll(_)]
        ));
        assert!(matches!(
            press(Key::ArrowUp, Modifiers::CTRL)[..],
            [Action::Pan(_)]
        ));
    }
}
