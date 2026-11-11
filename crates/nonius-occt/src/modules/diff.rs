//! Разница двух тел булевыми операциями: добавленный и убранный материал.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use super::{deflection, ANGULAR};
use crate::render::{colors, render, Image, Layer, LayerKind, Mesh, View};
use crate::shape::Shape;
use crate::OcctError;

/// Файл и его состояние: что было посчитано.
type Seen = (PathBuf, Option<(u64, SystemTime)>);
/// Какие файлы сравнивали и с каким допуском.
type DiffKey = (Seen, Seen, u64);
/// Запомненная разница.
type Known = Option<(DiffKey, Rc<GeometryDiff>)>;

thread_local! {
    /// Последняя разница на этом потоке. Сравнение и сцена просят одну и ту
    /// же пару подряд, а булева операция — самая долгая часть работы. Тела
    /// OpenCascade нельзя передавать между потоками, поэтому запоминает поток.
    static LAST: RefCell<Known> = const { RefCell::new(None) };
}

fn seen(path: &Path) -> Seen {
    let state = fs::metadata(path)
        .ok()
        .and_then(|m| Some((m.len(), m.modified().ok()?)));
    (path.to_path_buf(), state)
}

/// Забыть запомненную разницу. Вызывать, пока OpenCascade занят этим потоком:
/// при выходе потока тела освободились бы рядом с работой другого.
pub fn forget_diff() {
    LAST.with(|l| l.borrow_mut().take());
}

/// Разница двух STEP-файлов; подряд для той же пары считается один раз.
pub(super) fn shared_diff(
    before: &Path,
    after: &Path,
    tolerance_mm: f64,
) -> std::result::Result<Rc<GeometryDiff>, OcctError> {
    let key = (seen(before), seen(after), tolerance_mm.to_bits());
    let known = LAST.with(|l| {
        l.borrow()
            .as_ref()
            .filter(|(k, _)| *k == key)
            .map(|(_, d)| d.clone())
    });
    if let Some(diff) = known {
        return Ok(diff);
    }
    let diff = Rc::new(GeometryDiff::of_files(before, after, tolerance_mm)?);
    LAST.with(|l| *l.borrow_mut() = Some((key, diff.clone())));
    Ok(diff)
}

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
        let (vb, va) = (before.volume(), after.volume());
        let scale = vb.max(va);
        let threshold = (scale * 1e-6).max(tolerance_mm.powi(3));

        // Три пути, от быстрого к надёжному: только изменённые места, одно
        // пересечение тел целиком, два отдельных вычитания с допуском. Первые
        // два проверяются балансом «было + добавлено − убрано = стало».
        let measured = |(added, removed): (Shape, Shape)| {
            let (a, r) = (added.volume(), removed.volume());
            (added, a, removed, r)
        };
        let balanced = |&(_, a, _, r): &(Shape, f64, Shape, f64)| {
            (vb + a - r - va).abs() <= (scale * 1e-4).max(threshold * 10.0)
        };
        let attempt = |local: bool| {
            before
                .diff(&after, local)
                .ok()
                .map(measured)
                .filter(balanced)
        };
        let (added, a, removed, r) = match attempt(true).or_else(|| attempt(false)) {
            Some(found) => found,
            None => {
                let fuzzy = tolerance_mm.max(0.0);
                measured((after.cut(&before, fuzzy)?, before.cut(&after, fuzzy)?))
            }
        };
        Ok(Self {
            added_volume: a,
            removed_volume: r,
            added_pieces: added.solid_count(),
            removed_pieces: removed.solid_count(),
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
        let defl = deflection(self.after.rough_bbox().or_else(|| self.before.rough_bbox()));
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
