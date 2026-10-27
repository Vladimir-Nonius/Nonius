//! Растровая отрисовка сеток: превью детали и цветная подсветка изменений.
//!
//! Свой маленький рендер с z-буфером, без видеокарты и графических библиотек:
//! работает на сервере, в CI и на любой ОС. Вид — изометрия, как в CAD.
//!
//! Подсветка: неизменённый материал рисуется полупрозрачным серым «рентгеном»,
//! добавленный — зелёным, убранный — красным. Изменения видны даже внутри
//! детали (отверстие, внутренний паз), потому что серое их не закрывает.

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

/// Пиксель буфера одного вида слоёв.
#[derive(Clone, Copy)]
struct Frag {
    depth: f32,
    color: [f32; 3],
}

struct Buffer {
    w: usize,
    h: usize,
    frags: Vec<Option<Frag>>,
}

impl Buffer {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            frags: vec![None; w * h],
        }
    }
}

struct Camera {
    cos_a: f32,
    sin_a: f32,
    cos_e: f32,
    sin_e: f32,
    center: [f32; 3],
    scale: f32,
    offset: [f32; 2],
}

impl Camera {
    /// Точка модели → (x, y, глубина) в пикселях; глубина растёт от камеры.
    fn project(&self, p: [f32; 3]) -> [f32; 3] {
        let x = p[0] - self.center[0];
        let y = p[1] - self.center[1];
        let z = p[2] - self.center[2];
        let x1 = x * self.cos_a - y * self.sin_a;
        let y1 = x * self.sin_a + y * self.cos_a;
        let sx = x1;
        let sy = z * self.cos_e + y1 * self.sin_e;
        let depth = y1 * self.cos_e - z * self.sin_e;
        [
            self.offset[0] + sx * self.scale,
            self.offset[1] - sy * self.scale,
            depth,
        ]
    }
}

/// Нарисовать слои. Пустой список или пустые сетки дают прозрачную картинку.
pub fn render(layers: &[Layer<'_>], view: &View) -> Image {
    let ss = view.supersample.max(1);
    let w = (view.width * ss) as usize;
    let h = (view.height * ss) as usize;

    let mut camera = Camera {
        cos_a: view.azimuth_deg.to_radians().cos(),
        sin_a: view.azimuth_deg.to_radians().sin(),
        cos_e: view.elevation_deg.to_radians().cos(),
        sin_e: view.elevation_deg.to_radians().sin(),
        center: [0.0; 3],
        scale: 1.0,
        offset: [0.0; 2],
    };

    // Центр и масштаб по всем слоям, чтобы деталь заняла кадр с полями.
    let points = || layers.iter().flat_map(|l| l.mesh.positions.iter().copied());
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for p in points() {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    if lo[0] > hi[0] {
        return downsample(&Buffer::new(w, h), &Buffer::new(w, h), view, ss, 0.0);
    }
    // Совпадающие грани (верх отверстия = верх детали) не должны рябить:
    // серое считается «спереди», только если ближе заметно.
    let diagonal =
        ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
    let depth_eps = diagonal * 2e-3;
    camera.center = [
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    ];
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
    );
    for p in points() {
        let q = camera.project(p);
        min_x = min_x.min(q[0]);
        max_x = max_x.max(q[0]);
        min_y = min_y.min(q[1]);
        max_y = max_y.max(q[1]);
    }
    let margin = 0.08;
    let span_x = (max_x - min_x).max(1e-6);
    let span_y = (max_y - min_y).max(1e-6);
    camera.scale = ((w as f32 * (1.0 - 2.0 * margin)) / span_x)
        .min((h as f32 * (1.0 - 2.0 * margin)) / span_y);
    camera.offset = [
        w as f32 / 2.0 - (min_x + max_x) / 2.0 * camera.scale,
        h as f32 / 2.0 - (min_y + max_y) / 2.0 * camera.scale,
    ];

    let mut context = Buffer::new(w, h);
    let mut highlight = Buffer::new(w, h);
    for layer in layers {
        let target = match layer.kind {
            LayerKind::Context => &mut context,
            LayerKind::Highlight => &mut highlight,
        };
        draw_mesh(target, &camera, layer.mesh, layer.color);
    }
    downsample(&context, &highlight, view, ss, depth_eps)
}

/// Направление на источник света в координатах вида (x вправо, y вниз, глубина от камеры).
const LIGHT: [f32; 3] = [-0.45, -0.6, -0.66];

fn draw_mesh(buf: &mut Buffer, cam: &Camera, mesh: &Mesh, color: [u8; 3]) {
    let base = [color[0] as f32, color[1] as f32, color[2] as f32];
    let light = normalize(LIGHT);
    for t in &mesh.triangles {
        let Some(v) = t
            .iter()
            .map(|&i| mesh.positions.get(i as usize).map(|&p| cam.project(p)))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let (a, b, c) = (v[0], v[1], v[2]);

        // Освещение по нормали грани (двустороннее — ориентация не важна).
        let e1 = sub(b, a);
        let e2 = sub(c, a);
        let n = normalize(cross(e1, e2));
        let shade = 0.38 + 0.62 * dot(n, light).abs();
        let col = [base[0] * shade, base[1] * shade, base[2] * shade];

        let area = edge(a, b, c);
        if area.abs() < 1e-9 {
            continue;
        }
        let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
        let x1 = (a[0].max(b[0]).max(c[0]).ceil() as usize).min(buf.w.saturating_sub(1));
        let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
        let y1 = (a[1].max(b[1]).max(c[1]).ceil() as usize).min(buf.h.saturating_sub(1));
        if x0 > x1 || y0 > y1 {
            continue;
        }
        for py in y0..=y1 {
            for px in x0..=x1 {
                let p = [px as f32 + 0.5, py as f32 + 0.5, 0.0];
                let w0 = edge(b, c, p) / area;
                let w1 = edge(c, a, p) / area;
                let w2 = edge(a, b, p) / area;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let depth = w0 * a[2] + w1 * b[2] + w2 * c[2];
                let slot = &mut buf.frags[py * buf.w + px];
                if slot.is_none_or(|f| depth < f.depth) {
                    *slot = Some(Frag { depth, color: col });
                }
            }
        }
    }
}

/// Доля «рентгена»: насколько серое поверх изменения его приглушает.
const XRAY: f32 = 0.5;

fn downsample(context: &Buffer, highlight: &Buffer, view: &View, ss: u32, eps: f32) -> Image {
    let (w, h) = (view.width as usize, view.height as usize);
    let ss = ss as usize;
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let i = (y * ss + sy) * context.w + (x * ss + sx);
                    if let Some(c) = composite(context.frags[i], highlight.frags[i], eps) {
                        acc[0] += c[0];
                        acc[1] += c[1];
                        acc[2] += c[2];
                        acc[3] += 1.0;
                    }
                }
            }
            let o = (y * w + x) * 4;
            if acc[3] > 0.0 {
                for k in 0..3 {
                    rgba[o + k] = (acc[k] / acc[3]).round().clamp(0.0, 255.0) as u8;
                }
                rgba[o + 3] = (acc[3] / (ss * ss) as f32 * 255.0).round() as u8;
            }
        }
    }
    Image {
        width: view.width,
        height: view.height,
        rgba,
    }
}

fn composite(context: Option<Frag>, highlight: Option<Frag>, eps: f32) -> Option<[f32; 3]> {
    match (context, highlight) {
        (None, None) => None,
        (Some(c), None) => Some(c.color),
        (None, Some(hl)) => Some(hl.color),
        (Some(c), Some(hl)) => {
            if c.depth < hl.depth - eps {
                Some([
                    hl.color[0] * (1.0 - XRAY) + c.color[0] * XRAY,
                    hl.color[1] * (1.0 - XRAY) + c.color[1] * XRAY,
                    hl.color[2] * (1.0 - XRAY) + c.color[2] * XRAY,
                ])
            } else {
                Some(hl.color)
            }
        }
    }
}

fn edge(a: [f32; 3], b: [f32; 3], p: [f32; 3]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt();
    if len < 1e-12 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
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
