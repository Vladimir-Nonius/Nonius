//! 3D-вид в окне: принимает мышь и клавиши, рисует кадр, показывает оси.

use eframe::egui::{self, Color32, PointerButton, Pos2, Rect, Sense, Stroke, Vec2 as EVec2};
use eframe::egui_wgpu::RenderState;
use glam::{Vec2, Vec3};

use super::camera::{sphere, Camera, StdView};
use super::controls::{actions, Action, Input};
use super::gpu::{Frame, Gpu, Image, BACKGROUND};
use super::model::{Look, Mode, Model};
use super::pick::pick;

/// Длительность перехода между видами, с.
const ANIMATION: f64 = 0.3;

/// Что случилось в 3D-виде за кадр.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportEvent {
    /// Щёлкнули по месту изменения (номер куска) или мимо (`None`).
    Picked(Option<usize>),
}

/// 3D-вид.
pub struct Viewport {
    render: Option<RenderState>,
    gpu: Option<Gpu>,
    texture: Option<egui::TextureId>,
    model: Option<Model>,
    /// Камера сейчас.
    pub camera: Camera,
    animation: Option<(Camera, Camera, f64)>,
    last: Option<(Camera, Vec<super::gpu::Draw>, bool, [u32; 2])>,
    aspect: f32,
}

impl Viewport {
    /// Вид на устройстве окна. Без видеокарты (`None`) вид показывает надпись.
    pub fn new(render: Option<RenderState>) -> Self {
        let gpu = render
            .as_ref()
            .map(|r| Gpu::new(r.device.clone(), r.queue.clone()));
        Self {
            render,
            gpu,
            texture: None,
            model: None,
            camera: Camera::default(),
            animation: None,
            last: None,
            aspect: 1.5,
        }
    }

    /// Работает ли 3D.
    pub fn has_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    /// Показываемая модель.
    pub fn model(&self) -> Option<&Model> {
        self.model.as_ref()
    }

    /// Показать модель. `keep_camera` — оставить вид (та же деталь, другие версии).
    pub fn set_model(&mut self, model: Option<Model>, keep_camera: bool) {
        if let (Some(gpu), Some(m)) = (&mut self.gpu, &model) {
            gpu.upload(&m.parts);
        }
        self.last = None;
        self.model = model;
        if let (false, Some(m)) = (keep_camera, &self.model) {
            self.animation = None;
            self.camera.orientation = StdView::Iso.orientation();
            self.camera.fit(&m.bounds, self.aspect, 0.0);
        }
    }

    /// Плавно перейти к камере `to`.
    fn go(&mut self, ctx: &egui::Context, to: Camera) {
        self.animation = Some((self.camera, to, ctx.input(|i| i.time)));
        ctx.request_repaint();
    }

    /// Стандартный вид (кнопки над видом); `None` — тот же поворот, показать целиком.
    pub fn set_view(&mut self, ctx: &egui::Context, view: Option<StdView>) {
        if let Some(m) = &self.model {
            let mut to = self.camera;
            if let Some(v) = view {
                to.orientation = v.orientation();
            }
            to.fit(&m.bounds, self.aspect, 0.0);
            self.go(ctx, to);
        }
    }

    /// Приблизить к куску `part`, не меняя поворота.
    pub fn zoom_to(&mut self, ctx: &egui::Context, part: usize) {
        if let Some(m) = &self.model {
            if let Some(p) = m.parts.get(part) {
                let mut to = self.camera;
                // Место — примерно на треть вида, чтобы было видно, где оно на детали.
                let around = (p.bounds.diagonal() * 3.0).max(sphere(&m.bounds).1 * 0.4);
                to.fit(&p.bounds, self.aspect, around);
                self.go(ctx, to);
            }
        }
    }

    /// Последний кадр (для «Сохранить картинку»).
    pub fn image(&self) -> anyhow::Result<Option<Image>> {
        match &self.gpu {
            Some(g) => g.read_image(),
            None => Ok(None),
        }
    }

    /// Нарисовать вид на всё место `ui`.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        mode: Mode,
        look: &Look,
        selected: Option<usize>,
        wheel_reverse: bool,
    ) -> Option<ViewportEvent> {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        self.aspect = (rect.width() / rect.height().max(1.0)).max(1e-3);
        let [r, g, b] = BACKGROUND;
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_rgb(r, g, b));
        let (Some(gpu), Some(render), Some(model)) = (&mut self.gpu, &self.render, &self.model)
        else {
            return None;
        };
        let time = ui.input(|i| i.time);
        let mut event = None;

        // Ввод.
        let input = gather(ui, &response, rect);
        let pivot = Vec3::from(model.bounds.center());
        let size = model.bounds.diagonal();
        for a in actions(&input, wheel_reverse) {
            self.animation = None;
            let cam = &mut self.camera;
            match a {
                Action::Rotate(d) => cam.rotate(d, pivot),
                Action::Pan(px) => cam.pan(px, rect.height()),
                Action::Zoom { factor, at } => cam.zoom_at(factor, at, self.aspect, size),
                Action::Roll(angle) => cam.roll(angle, pivot),
                Action::View(v) => {
                    let mut to = *cam;
                    to.orientation = v.orientation();
                    to.fit(&model.bounds, self.aspect, 0.0);
                    self.animation = Some((*cam, to, time));
                }
                Action::Fit => {
                    let mut to = *cam;
                    to.fit(&model.bounds, self.aspect, 0.0);
                    self.animation = Some((*cam, to, time));
                }
            }
        }
        if response.clicked_by(PointerButton::Primary) {
            if let Some(ndc) = input.cursor {
                let (origin, dir) = self.camera.ray(ndc, self.aspect);
                let hit = pick(&model.parts, &model.visible(mode, look), origin, dir);
                let place = hit.filter(|&i| model.parts[i].role.is_change());
                event = Some(ViewportEvent::Picked(place));
            }
        }

        // Плавный переход.
        if let Some((from, to, start)) = self.animation {
            let t = ((time - start) / ANIMATION) as f32;
            self.camera = from.lerp(&to, t);
            if t >= 1.0 {
                self.animation = None;
            } else {
                ui.ctx().request_repaint();
            }
        }

        // Кадр.
        let ppp = ui.ctx().pixels_per_point();
        let px = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
        let draws = model.draws(mode, look, selected);
        let key = (self.camera, draws.clone(), look.edges, px);
        if self.last.as_ref() != Some(&key) || self.texture.is_none() {
            let frame = Frame {
                view_proj: self.camera.view_proj(self.aspect, &model.bounds),
                light: self.camera.orientation * Vec3::new(-0.35, 0.45, 1.0).normalize(),
                draws,
                edges: look.edges,
            };
            let (view, fresh) = gpu.render(&frame, px);
            let mut renderer = render.renderer.write();
            match self.texture {
                Some(id) if fresh => renderer.update_egui_texture_from_wgpu_texture(
                    &render.device,
                    view,
                    wgpu::FilterMode::Linear,
                    id,
                ),
                Some(_) => {}
                None => {
                    self.texture = Some(renderer.register_native_texture(
                        &render.device,
                        view,
                        wgpu::FilterMode::Linear,
                    ))
                }
            }
            self.last = Some(key);
        }
        if let Some(id) = self.texture {
            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            ui.painter().image(id, rect, uv, Color32::WHITE);
        }
        triad(ui.painter(), rect, &self.camera);
        event
    }
}

/// Собрать ввод за кадр. Клавиши — только когда не печатают в поле.
/// Ответ элемента (`response`) читается до `ui.input`: внутри него egui
/// держит блокировку, и повторный вход завис бы.
fn gather(ui: &egui::Ui, response: &egui::Response, rect: Rect) -> Input {
    let hovered = response.hovered() || response.dragged();
    let typing = ui.ctx().wants_keyboard_input();
    let button = [PointerButton::Middle, PointerButton::Primary]
        .into_iter()
        .find(|&b| response.dragged_by(b));
    let d = response.drag_delta();
    let cursor = response.hover_pos().map(|p| {
        let c = rect.center();
        Vec2::new(
            (p.x - c.x) / (rect.width() / 2.0),
            -(p.y - c.y) / (rect.height() / 2.0),
        )
    });
    let double_middle = response.double_clicked_by(PointerButton::Middle);
    ui.input(|i| Input {
        drag: Vec2::new(d.x, d.y),
        button,
        modifiers: i.modifiers,
        scroll: if hovered { i.raw_scroll_delta.y } else { 0.0 },
        pinch: if hovered { i.zoom_delta() } else { 1.0 },
        cursor,
        double_middle,
        keys: if typing {
            Vec::new()
        } else {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
                .collect()
        },
    })
}

/// Оси X, Y, Z в левом нижнем углу — как треугольник осей в SolidWorks.
fn triad(painter: &egui::Painter, rect: Rect, cam: &Camera) {
    let origin = rect.left_bottom() + EVec2::new(42.0, -42.0);
    let axes = [
        (Vec3::X, "X", Color32::from_rgb(214, 48, 49)),
        (Vec3::Y, "Y", Color32::from_rgb(34, 150, 66)),
        (Vec3::Z, "Z", Color32::from_rgb(40, 90, 200)),
    ];
    for (axis, name, color) in axes {
        let d = EVec2::new(axis.dot(cam.right()), -axis.dot(cam.up())) * 28.0;
        painter.line_segment([origin, origin + d], Stroke::new(2.0_f32, color));
        painter.text(
            origin + d * 1.25,
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::proportional(12.0),
            color,
        );
    }
}
