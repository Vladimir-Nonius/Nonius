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
/// Запомненный итог: разница или причина, по которой её не посчитать.
/// Отказ тоже запоминается: иначе сцена и картинка повторяли бы за
/// сравнением тот же долгий расчёт с тем же концом.
type Known = Option<(DiffKey, std::result::Result<Rc<GeometryDiff>, String>)>;

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
    let result = known.unwrap_or_else(|| {
        let fresh = GeometryDiff::of_files(before, after, tolerance_mm)
            .map(Rc::new)
            .map_err(|e| e.0);
        LAST.with(|l| *l.borrow_mut() = Some((key, fresh.clone())));
        fresh
    });
    result.map_err(OcctError)
}

/// Отдельный кусок добавленного или убранного материала.
pub struct Piece {
    /// Тело куска.
    pub shape: Shape,
    /// Его объём, мм³.
    pub volume: f64,
}

/// Куски материала без шума булевых операций, крупные первыми.
///
/// Шум — это плёнки вдоль совпадающих граней: они тоньше допуска. Поэтому
/// кусок судим по его толщине (объём, делённый на половину площади), а не по
/// доле от объёма детали: в большой детали доля прятала бы и настоящее
/// маленькое отверстие.
fn pieces(material: &Shape, tolerance_mm: f64) -> Vec<Piece> {
    let tol = tolerance_mm.max(1e-6);
    let mut kept: Vec<Piece> = material
        .solids()
        .into_iter()
        .filter_map(|shape| {
            let p = shape.props();
            let thickness = if p.area > 0.0 {
                2.0 * p.volume / p.area
            } else {
                0.0
            };
            (p.volume > tol.powi(3) && thickness > tol).then_some(Piece {
                shape,
                volume: p.volume,
            })
        })
        .collect();
    kept.sort_by(|a, b| b.volume.total_cmp(&a.volume));
    kept
}

/// Одна сетка из сеток всех кусков.
fn merged_mesh(pieces: &[Piece], defl: f64) -> Mesh {
    let mut all = Mesh::default();
    for piece in pieces {
        let m = piece.shape.mesh(defl, ANGULAR);
        let shift = all.positions.len() as u32;
        all.positions.extend(m.positions);
        all.triangles
            .extend(m.triangles.iter().map(|t| t.map(|i| i + shift)));
    }
    all
}

/// Разница двух тел: что добавлено, что убрано, что осталось.
pub struct GeometryDiff {
    /// Было.
    pub before: Shape,
    /// Стало.
    pub after: Shape,
    /// Добавленный материал («стало» минус «было») по кускам, крупные первыми.
    pub added: Vec<Piece>,
    /// Убранный материал («было» минус «стало») по кускам, крупные первыми.
    pub removed: Vec<Piece>,
    /// Объём добавленного, мм³.
    pub added_volume: f64,
    /// Объём убранного, мм³.
    pub removed_volume: f64,
    /// Число отдельных добавленных кусков.
    pub added_pieces: u32,
    /// Число отдельных убранных кусков.
    pub removed_pieces: u32,
    /// Сравнение приблизительное: все три пути дали разницу, у которой
    /// «было + добавлено − убрано» расходится со «стало» на столько мм³.
    pub unsure: Option<f64>,
    /// В одном из файлов нет твёрдых тел (только поверхности): объёмы
    /// вычитать не из чего, форма не сравнивалась.
    pub no_solids: bool,
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
        if before.solid_count() == 0 || after.solid_count() == 0 {
            return Ok(Self {
                before,
                after,
                added: Vec::new(),
                removed: Vec::new(),
                added_volume: 0.0,
                removed_volume: 0.0,
                added_pieces: 0,
                removed_pieces: 0,
                unsure: None,
                no_solids: true,
            });
        }
        let (vb, va) = (before.volume(), after.volume());
        // Насколько «было + добавлено − убрано» может не сойтись со «стало»
        // из-за одной лишь точности расчёта объёмов.
        let slack = (vb.max(va) * 1e-4).max(tolerance_mm.powi(3) * 10.0);
        let off =
            |(added, removed): &(Shape, Shape)| (vb + added.volume() - removed.volume() - va).abs();

        // Три пути, от быстрого к надёжному: только изменённые места, одно
        // пересечение тел целиком, два отдельных вычитания с допуском. Каждый
        // проверяется балансом объёмов; не сошёлся и последний — разница
        // показывается, но с пометкой «приблизительно».
        let attempt = |local: bool| before.diff(&after, local).ok().filter(|d| off(d) <= slack);
        let ((added, removed), unsure) = match attempt(true).or_else(|| attempt(false)) {
            Some(found) => (found, None),
            None => {
                let fuzzy = tolerance_mm.max(0.0);
                let cuts = (after.cut(&before, fuzzy)?, before.cut(&after, fuzzy)?);
                let off = off(&cuts);
                (cuts, (off > slack).then_some(off))
            }
        };
        let added = pieces(&added, tolerance_mm);
        let removed = pieces(&removed, tolerance_mm);
        Ok(Self {
            added_volume: added.iter().map(|p| p.volume).sum(),
            removed_volume: removed.iter().map(|p| p.volume).sum(),
            added_pieces: added.len() as u32,
            removed_pieces: removed.len() as u32,
            unsure,
            no_solids: false,
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
        !self.added.is_empty()
    }

    /// Убран ли материал.
    pub fn has_removed(&self) -> bool {
        !self.removed.is_empty()
    }

    /// Нарисовать: деталь серым «рентгеном», добавленное зелёным, убранное красным.
    ///
    /// Фон — новая версия целиком: добавленное лежит внутри неё и на совпадающих
    /// гранях выигрывает у серого, убранное — снаружи и видно сквозь серое.
    pub fn render(&self, view: &View) -> std::result::Result<Image, OcctError> {
        let defl = deflection(self.after.rough_bbox().or_else(|| self.before.rough_bbox()));
        let common_mesh = self.after.mesh(defl, ANGULAR);
        let added_mesh = merged_mesh(&self.added, defl);
        let removed_mesh = merged_mesh(&self.removed, defl);
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
