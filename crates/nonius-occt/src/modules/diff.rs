//! Разница двух тел булевыми операциями: добавленный и убранный материал.

use std::path::Path;

use super::{deflection, ANGULAR};
use crate::render::{colors, render, Image, Layer, LayerKind, Mesh, View};
use crate::shape::Shape;
use crate::OcctError;

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

        // Быстрый путь: одно общее пересечение. Проверяем баланс
        // «было + добавлено − убрано = стало»; если не сходится — медленный,
        // но надёжный путь из двух отдельных вычитаний с допуском.
        let fast = before.diff(&after).ok().and_then(|(added, removed)| {
            let (a, r) = (added.volume(), removed.volume());
            let mismatch = (vb + a - r - va).abs();
            let ok = a >= -threshold
                && r >= -threshold
                && mismatch <= (scale * 1e-4).max(threshold * 10.0);
            ok.then_some((added, a, removed, r))
        });
        let (added, a, removed, r) = match fast {
            Some(found) => found,
            None => {
                let fuzzy = tolerance_mm.max(0.0);
                let (added, removed) = (after.cut(&before, fuzzy)?, before.cut(&after, fuzzy)?);
                let (a, r) = (added.volume(), removed.volume());
                (added, a, removed, r)
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
        let defl = deflection(self.after.rough_bbox().or(self.before.rough_bbox()));
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
