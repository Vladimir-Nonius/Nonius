//! Что показывать в 3D: режимы «Было / Изменения / Стало», цвета, выбранное место.

use nonius_scene::{Bounds, Role, Scene};

use super::gpu::{Draw, Style};
use super::mesh::{prepare, Part};

/// Режим показа.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Старая версия: серая, убранное — красным.
    Before,
    /// Новая версия прозрачной (или серой), добавленное и убранное — цветом.
    #[default]
    Changes,
    /// Новая версия: серая, добавленное — зелёным.
    After,
}

/// Настройки показа, которые человек переключает в правой панели.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// Серую деталь в режиме «Изменения» — прозрачной.
    pub xray: bool,
    /// Рисовать рёбра.
    pub edges: bool,
    /// Показывать добавленное.
    pub added: bool,
    /// Показывать убранное.
    pub removed: bool,
    /// Показывать сдвинутое.
    pub moved: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            xray: true,
            edges: true,
            added: true,
            removed: true,
            moved: true,
        }
    }
}

impl Look {
    /// Включена ли роль переключателем в легенде.
    pub fn shows(&self, role: Role) -> bool {
        match role {
            Role::After | Role::Before => true,
            Role::Added => self.added,
            Role::Removed => self.removed,
            Role::Moved => self.moved,
        }
    }
}

/// Непрозрачность серой детали в «рентгене».
const GHOST_ALPHA: u8 = 80;

/// Сцена, подготовленная к показу.
#[derive(Debug, Clone)]
pub struct Model {
    /// Куски по порядку сцены.
    pub parts: Vec<Part>,
    /// Габариты всей сцены.
    pub bounds: Bounds,
    /// Есть ли изменения.
    pub has_changes: bool,
}

impl Model {
    /// Подготовить сцену. `None` — показать нечего.
    pub fn new(scene: &Scene) -> Option<Self> {
        let parts = prepare(scene);
        let bounds = parts.iter().map(|p| p.bounds).reduce(Bounds::union)?;
        Some(Self {
            has_changes: parts.iter().any(|p| p.role.is_change()),
            parts,
            bounds,
        })
    }

    /// Номера кусков-мест изменений в порядке списка.
    pub fn places(&self) -> Vec<usize> {
        (0..self.parts.len())
            .filter(|&i| self.parts[i].role.is_change())
            .collect()
    }

    /// Виден ли кусок в режиме.
    fn role_in(mode: Mode, role: Role) -> Option<Style> {
        match (mode, role) {
            (Mode::Changes, Role::After) => Some(Style::Ghost),
            (Mode::Changes, Role::Added | Role::Removed | Role::Moved) => Some(Style::Solid),
            (Mode::Before, Role::Before) => Some(Style::Behind),
            (Mode::Before, Role::Removed) => Some(Style::Solid),
            (Mode::After, Role::After) => Some(Style::Behind),
            (Mode::After, Role::Added | Role::Moved) => Some(Style::Solid),
            _ => None,
        }
    }

    /// Что рисовать. Без изменений — просто деталь.
    pub fn draws(&self, mode: Mode, look: &Look, selected: Option<usize>) -> Vec<Draw> {
        let mode = if self.has_changes { mode } else { Mode::After };
        let mut out = Vec::new();
        for (i, p) in self.parts.iter().enumerate() {
            let Some(mut style) = Self::role_in(mode, p.role) else {
                continue;
            };
            if !look.shows(p.role) {
                continue;
            }
            let [r, g, b] = p.role.color();
            let mut color = [r, g, b, 255];
            if style == Style::Ghost {
                if look.xray {
                    color[3] = GHOST_ALPHA;
                } else {
                    style = Style::Behind;
                }
            }
            if !self.has_changes {
                style = Style::Solid;
            }
            if selected == Some(i) {
                color = lighten(color);
            }
            out.push(Draw {
                part: i,
                color,
                style,
            });
        }
        out
    }

    /// Какие куски видны (для выбора щелчком).
    pub fn visible(&self, mode: Mode, look: &Look) -> Vec<bool> {
        let mut v = vec![false; self.parts.len()];
        for d in self.draws(mode, look, None) {
            v[d.part] = true;
        }
        v
    }
}

/// Выбранное место — светлее.
fn lighten(c: [u8; 4]) -> [u8; 4] {
    let up = |x: u8| x + ((255 - x) as u16 / 4) as u8;
    [up(c[0]), up(c[1]), up(c[2]), c[3]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::mesh::tests::cube_mesh;
    use glam::Vec3;
    use nonius_scene::Region;

    fn scene(roles: &[Role]) -> Scene {
        Scene {
            regions: roles
                .iter()
                .enumerate()
                .map(|(i, &role)| Region {
                    role,
                    volume_mm3: 1.0,
                    mesh: cube_mesh(Vec3::X * (i as f32 * 20.0), 10.0),
                })
                .collect(),
        }
    }

    fn shown(m: &Model, mode: Mode, look: &Look) -> Vec<(Role, Style)> {
        m.draws(mode, look, None)
            .iter()
            .map(|d| (m.parts[d.part].role, d.style))
            .collect()
    }

    #[test]
    fn modes_show_the_right_pieces() {
        use Role::*;
        let m = Model::new(&scene(&[After, Before, Added, Removed])).unwrap();
        let look = Look::default();
        assert_eq!(
            shown(&m, Mode::Changes, &look),
            [
                (After, Style::Ghost),
                (Added, Style::Solid),
                (Removed, Style::Solid)
            ]
        );
        assert_eq!(
            shown(&m, Mode::Before, &look),
            [(Before, Style::Behind), (Removed, Style::Solid)]
        );
        assert_eq!(
            shown(&m, Mode::After, &look),
            [(After, Style::Behind), (Added, Style::Solid)]
        );
        let solid = Look {
            xray: false,
            removed: false,
            ..look
        };
        assert_eq!(
            shown(&m, Mode::Changes, &solid),
            [(After, Style::Behind), (Added, Style::Solid)]
        );
        assert_eq!(m.places(), [2, 3]);
        assert_eq!(m.visible(Mode::After, &look), [true, false, true, false]);
    }

    #[test]
    fn unchanged_part_is_just_shown() {
        let m = Model::new(&scene(&[Role::After])).unwrap();
        assert!(!m.has_changes);
        for mode in [Mode::Before, Mode::Changes, Mode::After] {
            let d = m.draws(mode, &Look::default(), None);
            assert_eq!(d.len(), 1);
            assert_eq!((d[0].style, d[0].color[3]), (Style::Solid, 255));
        }
        assert!(Model::new(&Scene::default()).is_none());
    }

    #[test]
    fn selected_place_is_lighter() {
        let m = Model::new(&scene(&[Role::After, Role::Added])).unwrap();
        let d = m.draws(Mode::Changes, &Look::default(), Some(1));
        let green = Role::Added.color();
        assert!(d[1].color[0] > green[0] && d[1].color[2] > green[2]);
    }
}
