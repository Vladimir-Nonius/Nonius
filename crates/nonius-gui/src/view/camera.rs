//! Камера 3D-вида: ортогональная проекция, виды и движения как в SolidWorks.
//!
//! Оси — как в файле модели: у SolidWorks Y смотрит вверх, вид «Спереди»
//! смотрит вдоль −Z. Единицы — миллиметры.

use glam::{Mat3, Mat4, Quat, Vec2, Vec3};
use nonius_scene::Bounds;

/// Стандартный вид (Ctrl+1…7 в SolidWorks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdView {
    /// Спереди, Ctrl+1.
    Front,
    /// Сзади, Ctrl+2.
    Back,
    /// Слева, Ctrl+3.
    Left,
    /// Справа, Ctrl+4.
    Right,
    /// Сверху, Ctrl+5.
    Top,
    /// Снизу, Ctrl+6.
    Bottom,
    /// Изометрия, Ctrl+7.
    Iso,
}

impl StdView {
    /// Виды в порядке клавиш Ctrl+1…7.
    pub const ALL: [StdView; 7] = [
        StdView::Front,
        StdView::Back,
        StdView::Left,
        StdView::Right,
        StdView::Top,
        StdView::Bottom,
        StdView::Iso,
    ];

    /// Поворот камеры: куда смотрят её оси «вправо», «вверх», «на зрителя».
    pub fn orientation(self) -> Quat {
        use std::f32::consts::FRAC_PI_2;
        match self {
            StdView::Front => Quat::IDENTITY,
            StdView::Back => Quat::from_rotation_y(2.0 * FRAC_PI_2),
            StdView::Right => Quat::from_rotation_y(FRAC_PI_2),
            StdView::Left => Quat::from_rotation_y(-FRAC_PI_2),
            StdView::Top => Quat::from_rotation_x(-FRAC_PI_2),
            StdView::Bottom => Quat::from_rotation_x(FRAC_PI_2),
            StdView::Iso => {
                let back = Vec3::ONE.normalize();
                let right = Vec3::Y.cross(back).normalize();
                let up = back.cross(right);
                Quat::from_mat3(&Mat3::from_cols(right, up, back))
            }
        }
    }
}

/// Камера. `scale` — половина высоты вида в миллиметрах.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Точка в центре вида.
    pub target: Vec3,
    /// Поворот камеры (из её осей в оси модели).
    pub orientation: Quat,
    /// Половина высоты вида, мм.
    pub scale: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            orientation: StdView::Iso.orientation(),
            scale: 50.0,
        }
    }
}

/// Самое мелкое и самое крупное приближение относительно размера модели.
const MIN_SCALE: f32 = 1e-3;
const MAX_SCALE: f32 = 50.0;

impl Camera {
    /// Направление «вправо» на экране, в осях модели.
    pub fn right(&self) -> Vec3 {
        self.orientation * Vec3::X
    }

    /// Направление «вверх» на экране.
    pub fn up(&self) -> Vec3 {
        self.orientation * Vec3::Y
    }

    /// Направление взгляда (от зрителя в экран).
    pub fn forward(&self) -> Vec3 {
        self.orientation * Vec3::NEG_Z
    }

    /// Матрица «модель → экран» для вида `aspect` (ширина / высота).
    /// Глубина берётся по габаритам `scene`, чтобы ничего не обрезалось.
    pub fn view_proj(&self, aspect: f32, scene: &Bounds) -> Mat4 {
        let (center, radius) = sphere(scene);
        let depth = (center - self.target).dot(self.forward()).abs() + radius * 1.5 + 1.0;
        let eye = self.target - self.forward() * depth;
        let view = Mat4::look_to_rh(eye, self.forward(), self.up());
        let (h, w) = (self.scale, self.scale * aspect.max(1e-3));
        Mat4::orthographic_rh(-w, w, -h, h, 0.0, depth * 2.0) * view
    }

    /// Луч из точки экрана: `ndc` — от −1 до 1, Y вверх. Возвращает точку
    /// на плоскости через `target` и направление взгляда.
    pub fn ray(&self, ndc: Vec2, aspect: f32) -> (Vec3, Vec3) {
        let p = self.target
            + self.right() * (ndc.x * self.scale * aspect)
            + self.up() * (ndc.y * self.scale);
        (p, self.forward())
    }

    /// Повернуть вокруг осей экрана (средняя кнопка мыши): `delta` — углы
    /// в радианах вокруг вертикали и горизонтали экрана; `pivot` — центр вращения.
    pub fn rotate(&mut self, delta: Vec2, pivot: Vec3) {
        let turn = Quat::from_axis_angle(self.up(), -delta.x)
            * Quat::from_axis_angle(self.right(), -delta.y);
        self.turn(turn, pivot);
    }

    /// Повернуть вокруг оси взгляда (Alt+стрелки).
    pub fn roll(&mut self, angle: f32, pivot: Vec3) {
        self.turn(Quat::from_axis_angle(self.forward(), angle), pivot);
    }

    fn turn(&mut self, turn: Quat, pivot: Vec3) {
        self.orientation = (turn * self.orientation).normalize();
        self.target = pivot + turn * (self.target - pivot);
    }

    /// Сдвинуть изображение на `pixels` (Y вниз) в виде высотой `height` пикселей.
    pub fn pan(&mut self, pixels: Vec2, height: f32) {
        let mm = 2.0 * self.scale / height.max(1.0);
        self.target += (-self.right() * pixels.x + self.up() * pixels.y) * mm;
    }

    /// Приблизить в `factor` раз (меньше 1 — отдалить), не сдвигая точку под
    /// курсором `ndc`. `size` — размер модели, от него пределы масштаба.
    pub fn zoom_at(&mut self, factor: f32, ndc: Vec2, aspect: f32, size: f32) {
        let size = size.max(1e-3);
        let new = (self.scale / factor).clamp(size * MIN_SCALE, size * MAX_SCALE);
        let shift = self.right() * (ndc.x * aspect) + self.up() * ndc.y;
        self.target += shift * (self.scale - new);
        self.scale = new;
    }

    /// Показать `b` целиком, не меняя поворота. `min_size` — не приближать
    /// сильнее, чем до этого размера (для крошечных мест изменений).
    pub fn fit(&mut self, b: &Bounds, aspect: f32, min_size: f32) {
        let (r, u) = (self.right(), self.up());
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for c in corners(b) {
            let p = Vec2::new(c.dot(r), c.dot(u));
            lo = lo.min(p);
            hi = hi.max(p);
        }
        let mid = (lo + hi) / 2.0;
        let half = (hi - lo) / 2.0;
        let (center, _) = sphere(b);
        let along = center.dot(self.forward());
        self.target = r * mid.x + u * mid.y + self.forward() * along;
        let need = half.y.max(half.x / aspect.max(1e-3)).max(min_size / 2.0);
        self.scale = need.max(1e-3) * 1.1;
    }

    /// Плавный переход: `t` от 0 (`self`) до 1 (`to`).
    pub fn lerp(&self, to: &Camera, t: f32) -> Camera {
        if t <= 0.0 {
            return *self;
        }
        if t >= 1.0 {
            return *to;
        }
        let s = t * t * (3.0 - 2.0 * t);
        Camera {
            target: self.target.lerp(to.target, s),
            orientation: self.orientation.slerp(to.orientation, s),
            scale: (self.scale.ln() + (to.scale.ln() - self.scale.ln()) * s).exp(),
        }
    }
}

/// Центр и радиус описанной сферы.
pub fn sphere(b: &Bounds) -> (Vec3, f32) {
    let (lo, hi) = (Vec3::from(b.min), Vec3::from(b.max));
    ((lo + hi) / 2.0, (hi - lo).length() / 2.0)
}

fn corners(b: &Bounds) -> [Vec3; 8] {
    let (lo, hi) = (b.min, b.max);
    std::array::from_fn(|i| {
        Vec3::new(
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    fn cam(view: StdView) -> Camera {
        Camera {
            target: Vec3::ZERO,
            orientation: view.orientation(),
            scale: 10.0,
        }
    }

    fn cube() -> Bounds {
        Bounds {
            min: [0.0; 3],
            max: [60.0, 30.0, 8.0],
        }
    }

    #[test]
    fn views_like_solidworks() {
        // (вид, куда смотрим, что вверху экрана)
        let cases = [
            (StdView::Front, Vec3::NEG_Z, Vec3::Y),
            (StdView::Back, Vec3::Z, Vec3::Y),
            (StdView::Right, Vec3::NEG_X, Vec3::Y),
            (StdView::Left, Vec3::X, Vec3::Y),
            (StdView::Top, Vec3::NEG_Y, Vec3::NEG_Z),
            (StdView::Bottom, Vec3::Y, Vec3::Z),
        ];
        for (v, fwd, up) in cases {
            let c = cam(v);
            assert!(near(c.forward(), fwd), "{v:?}: {}", c.forward());
            assert!(near(c.up(), up), "{v:?}: {}", c.up());
        }
        let iso = cam(StdView::Iso);
        assert!(near(iso.forward(), -Vec3::ONE.normalize()));
        assert!(iso.up().y > 0.8, "в изометрии Y смотрит вверх");
        assert!(iso.right().x > 0.0 && iso.right().z < 0.0);
    }

    #[test]
    fn projection_maps_target_to_center() {
        let b = cube();
        let mut c = cam(StdView::Iso);
        c.fit(&b, 1.5, 0.0);
        let m = c.view_proj(1.5, &b);
        let p = m.project_point3(c.target);
        assert!(p.x.abs() < 1e-4 && p.y.abs() < 1e-4, "{p}");
        // Все углы внутри кадра и внутри глубины.
        for k in corners(&b) {
            let p = m.project_point3(k);
            assert!(p.x.abs() <= 1.0 && p.y.abs() <= 1.0, "{k} → {p}");
            assert!((0.0..=1.0).contains(&p.z), "глубина {k} → {p}");
        }
    }

    #[test]
    fn fit_fills_the_view() {
        let b = cube();
        let mut c = cam(StdView::Front);
        c.fit(&b, 2.0, 0.0);
        // Спереди деталь 60×30: по ширине 60/2/2 = 15, по высоте 15 → 15·1.1.
        assert!((c.scale - 16.5).abs() < 1e-3, "{}", c.scale);
        assert!(near(
            c.target.truncate().extend(0.0),
            Vec3::new(30.0, 15.0, 0.0)
        ));
        // Крошечное место не приближаем сильнее предела.
        let tiny = Bounds {
            min: [1.0; 3],
            max: [1.01; 3],
        };
        c.fit(&tiny, 1.0, 6.0);
        assert!((c.scale - 3.3).abs() < 1e-3, "{}", c.scale);
    }

    #[test]
    fn zoom_keeps_point_under_cursor() {
        let mut c = cam(StdView::Front);
        let ndc = Vec2::new(0.5, -0.25);
        let (before, _) = c.ray(ndc, 1.6);
        c.zoom_at(2.0, ndc, 1.6, 100.0);
        assert!((c.scale - 5.0).abs() < 1e-5);
        let (after, _) = c.ray(ndc, 1.6);
        assert!(near(before, after), "{before} → {after}");
        // Пределы масштаба.
        for _ in 0..100 {
            c.zoom_at(10.0, Vec2::ZERO, 1.0, 100.0);
        }
        assert!(c.scale >= 100.0 * MIN_SCALE - 1e-6);
    }

    #[test]
    fn pan_follows_the_mouse() {
        let mut c = cam(StdView::Front);
        // Высота вида 200 px = 20 мм: 10 px вправо — модель вправо на 1 мм,
        // значит центр вида уходит влево.
        c.pan(Vec2::new(10.0, 20.0), 200.0);
        assert!(near(c.target, Vec3::new(-1.0, 2.0, 0.0)), "{}", c.target);
    }

    #[test]
    fn rotate_turns_around_pivot() {
        let mut c = cam(StdView::Front);
        c.target = Vec3::new(10.0, 0.0, 0.0);
        let pivot = Vec3::ZERO;
        // Мышь вправо на четверть оборота: смотрим на деталь слева.
        c.rotate(Vec2::new(std::f32::consts::FRAC_PI_2, 0.0), pivot);
        assert!(near(c.forward(), Vec3::X), "{}", c.forward());
        assert!(near(c.up(), Vec3::Y));
        assert!(
            (c.target.length() - 10.0).abs() < 1e-4,
            "центр вида остался на месте"
        );
        c.roll(std::f32::consts::FRAC_PI_2, pivot);
        assert!(near(c.forward(), Vec3::X));
        assert!(!near(c.up(), Vec3::Y));
    }

    #[test]
    fn lerp_ends_exactly() {
        let a = cam(StdView::Front);
        let mut b = cam(StdView::Top);
        b.scale = 40.0;
        b.target = Vec3::ONE;
        assert_eq!(a.lerp(&b, 0.0), a);
        let end = a.lerp(&b, 1.0);
        assert!(near(end.forward(), b.forward()) && (end.scale - 40.0).abs() < 1e-3);
        let mid = a.lerp(&b, 0.5);
        assert!(
            (mid.scale - 20.0).abs() < 1e-3,
            "масштаб меняется плавно: {}",
            mid.scale
        );
    }
}
