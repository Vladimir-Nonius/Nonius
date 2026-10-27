//! Растровая отрисовка сеток: превью детали и цветная подсветка изменений.
//!
//! Свой маленький рендер с z-буфером, без видеокарты и графических библиотек:
//! работает на сервере, в CI и на любой ОС. Вид — изометрия, как в CAD.
//!
//! Подсветка: неизменённый материал рисуется полупрозрачным серым «рентгеном»,
//! добавленный — зелёным, убранный — красным. Изменения видны даже внутри
//! детали (отверстие, внутренний паз), потому что серое их не закрывает.

//!
//! Файлы: здесь — сетка, слои, вид, картинка; `raster.rs` — сама отрисовка
//! (камера, z-буфер, свет, смешивание слоёв).

mod raster;

pub use raster::render;

/// Треугольная сетка: вершины в мм и тройки индексов.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    /// Вершины.
    pub positions: Vec<[f32; 3]>,
    /// Треугольники — индексы вершин.
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    /// Пустая ли сетка.
    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }
}

/// Роль слоя на картинке.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    /// Фон-контекст: полупрозрачный, не закрывает изменения.
    Context,
    /// Изменение: непрозрачное.
    Highlight,
}

/// Слой картинки.
#[derive(Debug, Clone, Copy)]
pub struct Layer<'a> {
    /// Сетка.
    pub mesh: &'a Mesh,
    /// Цвет RGB.
    pub color: [u8; 3],
    /// Роль.
    pub kind: LayerKind,
}

/// Цвета подсветки. Те же, что в интерфейсе.
pub mod colors {
    /// Неизменённый материал.
    pub const UNCHANGED: [u8; 3] = [176, 182, 190];
    /// Добавленный материал.
    pub const ADDED: [u8; 3] = [34, 170, 76];
    /// Убранный материал.
    pub const REMOVED: [u8; 3] = [214, 48, 49];
    /// Сдвинутое без изменения формы.
    pub const MOVED: [u8; 3] = [240, 180, 20];
    /// Превью детали.
    pub const PREVIEW: [u8; 3] = [150, 170, 196];
}

/// Параметры вида.
#[derive(Debug, Clone, Copy)]
pub struct View {
    /// Ширина картинки, пикс.
    pub width: u32,
    /// Высота картинки, пикс.
    pub height: u32,
    /// Поворот вокруг вертикали, градусы.
    pub azimuth_deg: f32,
    /// Наклон камеры вниз, градусы.
    pub elevation_deg: f32,
    /// Сглаживание: во сколько раз крупнее рисовать перед уменьшением.
    pub supersample: u32,
}

impl Default for View {
    fn default() -> Self {
        Self {
            width: 960,
            height: 720,
            azimuth_deg: 45.0,
            elevation_deg: 35.264,
            supersample: 2,
        }
    }
}

/// Картинка RGBA, 8 бит на канал, строки сверху вниз.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    /// Ширина.
    pub width: u32,
    /// Высота.
    pub height: u32,
    /// Пиксели RGBA.
    pub rgba: Vec<u8>,
}

impl Image {
    /// Цвет пикселя.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    /// Сколько непрозрачных пикселей близки к цвету `color` (±`tol` по каналу).
    pub fn count_color(&self, color: [u8; 3], tol: u8) -> usize {
        self.rgba
            .chunks_exact(4)
            .filter(|p| p[3] > 200 && (0..3).all(|i| p[i].abs_diff(color[i]) <= tol))
            .count()
    }
}

/// Сетка прямоугольного параллелепипеда — для тестов и подписей.
pub fn box_mesh(min: [f32; 3], max: [f32; 3]) -> Mesh {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    let positions = vec![
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    let triangles = vec![
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    Mesh {
        positions,
        triangles,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scene_is_transparent() {
        let img = render(&[], &View::default());
        assert_eq!(img.width, 960);
        assert!(img.rgba.chunks_exact(4).all(|p| p[3] == 0));
    }

    #[test]
    fn part_fills_frame_with_margins() {
        let mesh = box_mesh([0.0, 0.0, 0.0], [40.0, 20.0, 5.0]);
        let view = View {
            width: 200,
            height: 150,
            ..View::default()
        };
        let img = render(
            &[Layer {
                mesh: &mesh,
                color: colors::PREVIEW,
                kind: LayerKind::Context,
            }],
            &view,
        );
        assert_eq!(img.pixel(0, 0)[3], 0, "углы — фон");
        assert!(img.pixel(100, 75)[3] > 200, "центр — деталь");
        let filled = img.rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(
            filled > 200 * 150 / 5,
            "деталь занимает заметную часть кадра: {filled}"
        );
    }

    #[test]
    fn inner_change_is_visible_through_context() {
        // Деталь-коробка и «убранный» объём внутри неё — как отверстие.
        let body = box_mesh([0.0, 0.0, 0.0], [40.0, 40.0, 10.0]);
        let hole = box_mesh([15.0, 15.0, 2.0], [25.0, 25.0, 8.0]);
        let img = render(
            &[
                Layer {
                    mesh: &body,
                    color: colors::UNCHANGED,
                    kind: LayerKind::Context,
                },
                Layer {
                    mesh: &hole,
                    color: colors::REMOVED,
                    kind: LayerKind::Highlight,
                },
            ],
            &View {
                width: 240,
                height: 180,
                ..View::default()
            },
        );
        let reddish = img
            .rgba
            .chunks_exact(4)
            .filter(|p| p[3] > 200 && p[0] as i32 - p[1] as i32 > 40)
            .count();
        assert!(
            reddish > 100,
            "убранное внутри видно сквозь серое: {reddish}"
        );
    }
}
