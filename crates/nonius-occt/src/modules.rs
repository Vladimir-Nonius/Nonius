//! Модули Nonius на OpenCascade: чтение STEP с контрольными числами,
//! сравнение геометрии и цветная подсветка изменений.

use std::path::{Path, PathBuf};

use nonius_core::compare::{Change, ChangeKind, ChangeReport, CompareContext};
use nonius_core::extension::{
    Comparator, ExtensionPoint, GeometrySource, Module, ModuleManifest, Renderer, Snapshot,
    API_VERSION,
};
use nonius_core::meta::{BoundingBox, ItemKind, PartMeta};
use nonius_core::{Error, Result};
use nonius_step::StepHeaderSource;

use crate::render::{colors, render, Image, Layer, LayerKind, Mesh, View};
use crate::shape::Shape;
use crate::{save_png, OcctError};

/// Имя модели в папке детали.
pub const MODEL_STEP: &str = "model.step";
/// Имя картинки изменений, которую рисует [`HighlightRenderer`].
pub const CHANGES_PNG: &str = "changes.png";

pub(crate) fn manifest(id: &str, name: &str, point: ExtensionPoint) -> ModuleManifest {
    ModuleManifest {
        id: id.into(),
        name: name.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        api: API_VERSION,
        points: vec![point],
        permissions: vec![],
    }
}

pub(crate) fn module_err(id: &str, e: OcctError) -> Error {
    Error::Module {
        module: id.into(),
        message: e.0,
    }
}

fn is_step(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("step") || e.eq_ignore_ascii_case("stp"))
}

/// Допуск сетки по размеру тела: мелкие детали — мельче.
pub(crate) fn deflection(props_bbox: Option<([f64; 3], [f64; 3])>) -> f64 {
    let diag = props_bbox
        .map(|(lo, hi)| {
            ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt()
        })
        .unwrap_or(100.0);
    (diag * 1.5e-3).max(0.005)
}

pub(crate) const ANGULAR: f64 = 0.3;

// ─── Источник геометрии ──────────────────────────────────────────────

/// Чтение STEP через OpenCascade: объём, площадь, центр масс, габариты, число
/// тел и граней. Сведения заголовка (CAD, автор, протокол) — как у `step-header`.
pub struct OcctStepSource {
    manifest: ModuleManifest,
}

impl OcctStepSource {
    /// Идентификатор модуля.
    pub const ID: &'static str = "step-occt";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: manifest(
                Self::ID,
                "Геометрия STEP (OpenCascade)",
                ExtensionPoint::GeometrySource,
            ),
        }
    }
}

impl Default for OcctStepSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for OcctStepSource {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl GeometrySource for OcctStepSource {
    fn can_read(&self, path: &Path) -> bool {
        is_step(path)
    }

    fn read(&self, path: &Path) -> Result<PartMeta> {
        let mut meta = StepHeaderSource::new()
            .read(path)
            .unwrap_or_else(|_| PartMeta::new(ItemKind::Part));
        let shape = Shape::read_step(path).map_err(|e| module_err(Self::ID, e))?;
        let p = shape.props();
        let g = &mut meta.geometry;
        g.volume_mm3 = Some(p.volume);
        g.area_mm2 = Some(p.area);
        g.center_of_mass = (p.volume > 0.0).then_some(p.center);
        g.bbox = p.bbox.map(|(min, max)| BoundingBox { min, max });
        g.bodies = Some(p.solids);
        g.faces = Some(p.faces);
        meta.source.module = Some(Self::ID.into());
        Ok(meta)
    }
}

// ─── Сравнение геометрии ─────────────────────────────────────────────

/// Разница двух тел: что добавлено, что убрано, что осталось.
pub struct GeometryDiff {
    /// Было.
    pub before: Shape,
    /// Стало.
    pub after: Shape,
    /// Добавленный материал: «стало» минус «было».
    pub added: Shape,
    /// Убранный материал: «было» минус «стало».
    pub removed: Shape,
    /// Объём добавленного, мм³.
    pub added_volume: f64,
    /// Объём убранного, мм³.
    pub removed_volume: f64,
    /// Число отдельных добавленных кусков.
    pub added_pieces: u32,
    /// Число отдельных убранных кусков.
    pub removed_pieces: u32,
    /// Порог: объём меньше него считается шумом, мм³.
    pub threshold: f64,
}

impl GeometryDiff {
    /// Сравнить два STEP-файла с допуском `tolerance_mm`.
    pub fn of_files(
        before: &Path,
        after: &Path,
        tolerance_mm: f64,
    ) -> std::result::Result<Self, OcctError> {
        let before = Shape::read_step(before)?;
        let after = Shape::read_step(after)?;
        Self::of_shapes(before, after, tolerance_mm)
    }

    /// Сравнить два тела.
    pub fn of_shapes(
        before: Shape,
        after: Shape,
        tolerance_mm: f64,
    ) -> std::result::Result<Self, OcctError> {
        let (vb, va) = (before.props().volume, after.props().volume);
        let scale = vb.max(va);
        let threshold = (scale * 1e-6).max(tolerance_mm.powi(3));

        // Быстрый путь: одно общее пересечение. Проверяем баланс
        // «было + добавлено − убрано = стало»; если не сходится — медленный,
        // но надёжный путь из двух отдельных вычитаний с допуском.
        let fast = before.diff(&after).ok().filter(|(added, removed)| {
            let (a, r) = (added.props().volume, removed.props().volume);
            let mismatch = (vb + a - r - va).abs();
            a >= -threshold && r >= -threshold && mismatch <= (scale * 1e-4).max(threshold * 10.0)
        });
        let (added, removed) = match fast {
            Some(pair) => pair,
            None => {
                let fuzzy = tolerance_mm.max(0.0);
                (after.cut(&before, fuzzy)?, before.cut(&after, fuzzy)?)
            }
        };
        let (a, r) = (added.props(), removed.props());
        Ok(Self {
            added_volume: a.volume,
            removed_volume: r.volume,
            added_pieces: a.solids,
            removed_pieces: r.solids,
            threshold,
            before,
            after,
            added,
            removed,
        })
    }

    /// Есть ли заметные изменения формы.
    pub fn changed(&self) -> bool {
        self.has_added() || self.has_removed()
    }

    /// Добавлен ли материал.
    pub fn has_added(&self) -> bool {
        self.added_volume > self.threshold
    }

    /// Убран ли материал.
    pub fn has_removed(&self) -> bool {
        self.removed_volume > self.threshold
    }

    /// Нарисовать: деталь серым «рентгеном», добавленное зелёным, убранное красным.
    ///
    /// Фон — новая версия целиком: добавленное лежит внутри неё и на совпадающих
    /// гранях выигрывает у серого, убранное — снаружи и видно сквозь серое.
    pub fn render(&self, view: &View) -> std::result::Result<Image, OcctError> {
        let defl = deflection(self.after.props().bbox.or(self.before.props().bbox));
        let common_mesh = self.after.mesh(defl, ANGULAR);
        let empty = Mesh::default();
        let added_mesh = if self.has_added() {
            self.added.mesh(defl, ANGULAR)
        } else {
            empty.clone()
        };
        let removed_mesh = if self.has_removed() {
            self.removed.mesh(defl, ANGULAR)
        } else {
            empty
        };
        Ok(render(
            &[
                Layer {
                    mesh: &common_mesh,
                    color: colors::UNCHANGED,
                    kind: LayerKind::Context,
                },
                Layer {
                    mesh: &added_mesh,
                    color: colors::ADDED,
                    kind: LayerKind::Highlight,
                },
                Layer {
                    mesh: &removed_mesh,
                    color: colors::REMOVED,
                    kind: LayerKind::Highlight,
                },
            ],
            view,
        ))
    }
}

/// Нарисовать превью одной детали из STEP.
pub fn preview(step: &Path, view: &View) -> std::result::Result<Image, OcctError> {
    let shape = Shape::read_step(step)?;
    let mesh = shape.mesh(deflection(shape.props().bbox), ANGULAR);
    Ok(render(
        &[Layer {
            mesh: &mesh,
            color: colors::PREVIEW,
            kind: LayerKind::Context,
        }],
        view,
    ))
}

pub(crate) fn model_pair(before: &Snapshot, after: &Snapshot) -> Option<(PathBuf, PathBuf)> {
    let a = before.dir.join(MODEL_STEP);
    let b = after.dir.join(MODEL_STEP);
    (a.is_file() && b.is_file()).then_some((a, b))
}

pub(crate) fn same_bytes(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) if ma.len() != mb.len() => false,
        _ => matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y),
    }
}

/// Сравнение формы через булевы операции: сколько материала добавлено и убрано.
pub struct GeometryComparator {
    manifest: ModuleManifest,
}

impl GeometryComparator {
    /// Идентификатор модуля.
    pub const ID: &'static str = "geometry-occt";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: manifest(
                Self::ID,
                "Сравнение формы (OpenCascade)",
                ExtensionPoint::Comparator,
            ),
        }
    }
}

impl Default for GeometryComparator {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for GeometryComparator {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

fn volume_text(volume: f64, pieces: u32) -> String {
    let v = if volume >= 100.0 {
        format!("{volume:.0} мм³")
    } else {
        format!("{volume:.2} мм³")
    };
    if pieces > 1 {
        format!("{v}, мест: {pieces}")
    } else {
        v
    }
}

impl Comparator for GeometryComparator {
    fn compare(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        ctx: &CompareContext,
    ) -> Result<ChangeReport> {
        let mut report = ChangeReport::empty(Self::ID);
        let Some((a, b)) = model_pair(before, after) else {
            return Ok(report);
        };
        if same_bytes(&a, &b) {
            return Ok(report);
        }
        let diff = GeometryDiff::of_files(&a, &b, ctx.tolerance_mm)
            .map_err(|e| module_err(Self::ID, e))?;
        if diff.has_added() {
            report.changes.push(Change {
                kind: ChangeKind::Added,
                subject: "Добавлен материал".into(),
                before: None,
                after: Some(volume_text(diff.added_volume, diff.added_pieces)),
            });
        }
        if diff.has_removed() {
            report.changes.push(Change {
                kind: ChangeKind::Removed,
                subject: "Убран материал".into(),
                before: Some(volume_text(diff.removed_volume, diff.removed_pieces)),
                after: None,
            });
        }
        Ok(report)
    }
}

/// Картинка изменений: `changes.png` с цветной подсветкой.
pub struct HighlightRenderer {
    manifest: ModuleManifest,
    view: View,
}

impl HighlightRenderer {
    /// Идентификатор модуля.
    pub const ID: &'static str = "highlight-occt";

    /// Создать модуль с видом по умолчанию.
    pub fn new() -> Self {
        Self {
            manifest: manifest(
                Self::ID,
                "Цветная подсветка изменений",
                ExtensionPoint::Renderer,
            ),
            view: View::default(),
        }
    }
}

impl Default for HighlightRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for HighlightRenderer {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl Renderer for HighlightRenderer {
    fn render(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        _report: &ChangeReport,
        out_dir: &Path,
    ) -> Result<Vec<PathBuf>> {
        let Some((a, b)) = model_pair(before, after) else {
            return Ok(Vec::new());
        };
        let diff = GeometryDiff::of_files(&a, &b, CompareContext::default().tolerance_mm)
            .map_err(|e| module_err(Self::ID, e))?;
        let img = diff
            .render(&self.view)
            .map_err(|e| module_err(Self::ID, e))?;
        std::fs::create_dir_all(out_dir).map_err(|e| Error::Io {
            path: out_dir.to_path_buf(),
            source: e,
        })?;
        let path = out_dir.join(CHANGES_PNG);
        save_png(&img, &path).map_err(|e| Error::Io {
            path: path.clone(),
            source: e,
        })?;
        Ok(vec![path])
    }
}
