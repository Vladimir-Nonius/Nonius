//! Безопасная обёртка над телом OpenCascade.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;
use std::ptr::NonNull;

use crate::render::Mesh;
use crate::OcctError;
use crate::{cache, ffi};

/// Чтение и запись STEP в OpenCascade держат общие настройки (`Interface_Static`)
/// и из двух потоков сразу ломаются («StepSelect_StepType»). Окно считает в
/// фоне, тесты идут параллельно — поэтому STEP читается и пишется по очереди.
fn step_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Контрольные числа тела. Единицы: мм, мм², мм³.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Props {
    /// Объём.
    pub volume: f64,
    /// Площадь поверхности.
    pub area: f64,
    /// Центр масс (при единичной плотности).
    pub center: [f64; 3],
    /// Габариты: минимальный и максимальный угол.
    pub bbox: Option<([f64; 3], [f64; 3])>,
    /// Число твёрдых тел.
    pub solids: u32,
    /// Число граней.
    pub faces: u32,
}

/// Тело (или несколько тел) OpenCascade.
pub struct Shape {
    ptr: NonNull<ffi::NoniusShape>,
}

impl Drop for Shape {
    fn drop(&mut self) {
        // SAFETY: указатель получен из прослойки и освобождается ровно один раз.
        unsafe { ffi::nonius_shape_free(self.ptr.as_ptr()) }
    }
}

const ERR_LEN: usize = 512;

fn c_path(path: &Path) -> Result<CString, OcctError> {
    let text = path
        .to_str()
        .ok_or_else(|| OcctError(format!("{}: путь не в UTF-8", path.display())))?;
    CString::new(text).map_err(|_| OcctError(format!("{}: нулевой символ в пути", path.display())))
}

fn error_text(buf: &[c_char]) -> String {
    // SAFETY: прослойка всегда завершает строку нулём внутри буфера.
    let text = unsafe { CStr::from_ptr(buf.as_ptr()) };
    let text = text.to_string_lossy().into_owned();
    if text.is_empty() {
        "ошибка OpenCascade".into()
    } else {
        text
    }
}

fn wrap(ptr: *mut ffi::NoniusShape, err: &[c_char], what: &str) -> Result<Shape, OcctError> {
    NonNull::new(ptr)
        .map(|ptr| Shape { ptr })
        .ok_or_else(|| OcctError(format!("{what}: {}", error_text(err))))
}

/// Версия OpenCascade, с которой собрана программа.
pub fn version() -> String {
    // SAFETY: функция возвращает статическую строку.
    unsafe { CStr::from_ptr(ffi::nonius_occt_version()) }
        .to_string_lossy()
        .into_owned()
}

impl Shape {
    /// Прочитать STEP. Единицы приводятся к мм. Повторное чтение того же
    /// содержимого берётся из кэша (см. `cache.rs`) — в сотни раз быстрее.
    pub fn read_step(path: &Path) -> Result<Self, OcctError> {
        let entry = cache::entry(path, &version());
        if let Some(shape) = entry.as_deref().and_then(Self::read_brep) {
            return Ok(shape);
        }
        let shape = Self::read_step_file(path)?;
        if let Some(entry) = &entry {
            cache::store(entry, |tmp| shape.write_brep(tmp));
        }
        Ok(shape)
    }

    fn read_step_file(path: &Path) -> Result<Self, OcctError> {
        let c = c_path(path)?;
        let mut err = [0 as c_char; ERR_LEN];
        let _guard = step_lock();
        // SAFETY: строки живут до конца вызова, буфер нужной длины.
        let ptr = unsafe { ffi::nonius_read_step(c.as_ptr(), err.as_mut_ptr(), ERR_LEN) };
        wrap(ptr, &err, &path.display().to_string())
    }

    fn read_brep(path: &Path) -> Option<Self> {
        let c = c_path(path).ok()?;
        // SAFETY: строка живёт до конца вызова.
        let ptr = unsafe { ffi::nonius_read_brep(c.as_ptr()) };
        NonNull::new(ptr).map(|ptr| Shape { ptr })
    }

    fn write_brep(&self, path: &Path) -> bool {
        let Ok(c) = c_path(path) else {
            return false;
        };
        // SAFETY: тело живое, строка живёт до конца вызова.
        unsafe { ffi::nonius_write_brep(self.ptr.as_ptr(), c.as_ptr()) == 1 }
    }

    /// Записать STEP (AP214, мм).
    pub fn write_step(&self, path: &Path) -> Result<(), OcctError> {
        let c = c_path(path)?;
        let mut err = [0 as c_char; ERR_LEN];
        let _guard = step_lock();
        // SAFETY: тело живое, строки живут до конца вызова.
        let ok = unsafe {
            ffi::nonius_write_step(self.ptr.as_ptr(), c.as_ptr(), err.as_mut_ptr(), ERR_LEN)
        };
        if ok == 1 {
            Ok(())
        } else {
            Err(OcctError(format!(
                "{}: {}",
                path.display(),
                error_text(&err)
            )))
        }
    }

    /// Контрольные числа.
    pub fn props(&self) -> Props {
        let mut p = ffi::NoniusProps::default();
        // SAFETY: тело живое, структура нужного размера.
        let ok = unsafe { ffi::nonius_shape_props(self.ptr.as_ptr(), &mut p) };
        if ok != 1 {
            return Props::default();
        }
        Props {
            volume: p.volume,
            area: p.area,
            center: p.center,
            bbox: (p.has_bbox == 1).then_some((p.bbox_min, p.bbox_max)),
            solids: p.solids,
            faces: p.faces,
        }
    }

    /// Только объём, мм³. Быстрее [`Shape::props`]: без площади и точных габаритов.
    pub fn volume(&self) -> f64 {
        // SAFETY: тело живое.
        unsafe { ffi::nonius_shape_volume(self.ptr.as_ptr()) }
    }

    /// Число твёрдых тел (отдельных кусков).
    pub fn solid_count(&self) -> u32 {
        // SAFETY: тело живое.
        unsafe { ffi::nonius_shape_solid_count(self.ptr.as_ptr()) }
    }

    /// Габариты с запасом — мгновенно, в отличие от точных в [`Shape::props`].
    /// Годятся для допуска сетки и вида, но не для паспорта детали.
    pub fn rough_bbox(&self) -> Option<([f64; 3], [f64; 3])> {
        let (mut lo, mut hi) = ([0.0; 3], [0.0; 3]);
        // SAFETY: тело живое, массивы по три числа.
        let ok = unsafe {
            ffi::nonius_shape_rough_bbox(self.ptr.as_ptr(), lo.as_mut_ptr(), hi.as_mut_ptr())
        };
        (ok == 1).then_some((lo, hi))
    }

    fn boolean(&self, other: &Shape, op: i32, fuzzy: f64, what: &str) -> Result<Shape, OcctError> {
        let mut err = [0 as c_char; ERR_LEN];
        // SAFETY: оба тела живые, буфер нужной длины.
        let ptr = unsafe {
            ffi::nonius_boolean(
                self.ptr.as_ptr(),
                other.ptr.as_ptr(),
                op,
                fuzzy,
                err.as_mut_ptr(),
                ERR_LEN,
            )
        };
        wrap(ptr, &err, what)
    }

    /// `self` минус `other`.
    pub fn cut(&self, other: &Shape, fuzzy: f64) -> Result<Shape, OcctError> {
        self.boolean(other, 0, fuzzy, "вычитание")
    }

    /// Общая часть.
    pub fn common(&self, other: &Shape, fuzzy: f64) -> Result<Shape, OcctError> {
        self.boolean(other, 1, fuzzy, "пересечение")
    }

    /// Объединение.
    pub fn fuse(&self, other: &Shape, fuzzy: f64) -> Result<Shape, OcctError> {
        self.boolean(other, 2, fuzzy, "объединение")
    }

    /// Разность версий за одно пересечение: `(добавлено, убрано)`, где
    /// добавлено = `after` − `self`, убрано = `self` − `after`. Быстрее двух
    /// [`Shape::cut`]: самое дорогое — пересечение — считается один раз.
    pub fn diff(&self, after: &Shape) -> Result<(Shape, Shape), OcctError> {
        let mut err = [0 as c_char; ERR_LEN];
        let mut added: *mut ffi::NoniusShape = std::ptr::null_mut();
        let mut removed: *mut ffi::NoniusShape = std::ptr::null_mut();
        // SAFETY: оба тела живые, указатели на результаты и буфер действительны.
        let ok = unsafe {
            ffi::nonius_diff(
                self.ptr.as_ptr(),
                after.ptr.as_ptr(),
                &mut added,
                &mut removed,
                err.as_mut_ptr(),
                ERR_LEN,
            )
        };
        if ok != 1 {
            return Err(OcctError(format!("сравнение: {}", error_text(&err))));
        }
        Ok((
            wrap(added, &err, "сравнение")?,
            wrap(removed, &err, "сравнение")?,
        ))
    }

    /// Треугольная сетка для отрисовки. `linear` — допуск, мм; `angular` — радианы.
    pub fn mesh(&self, linear: f64, angular: f64) -> Mesh {
        let mut m = ffi::NoniusMesh {
            positions: std::ptr::null_mut(),
            vertex_count: 0,
            indices: std::ptr::null_mut(),
            triangle_count: 0,
        };
        // SAFETY: тело живое; память сетки выделена прослойкой и освобождается ниже.
        let ok = unsafe { ffi::nonius_shape_mesh(self.ptr.as_ptr(), linear, angular, &mut m) };
        let mut out = Mesh::default();
        if ok == 1 && !m.positions.is_null() && !m.indices.is_null() {
            // SAFETY: прослойка выделила ровно столько элементов.
            let pos = unsafe { std::slice::from_raw_parts(m.positions, m.vertex_count * 3) };
            let idx = unsafe { std::slice::from_raw_parts(m.indices, m.triangle_count * 3) };
            out.positions = pos.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
            out.triangles = idx
                .chunks_exact(3)
                .map(|c| [c[0], c[1], c[2]])
                .filter(|t| t.iter().all(|&i| (i as usize) < m.vertex_count))
                .collect();
        }
        // SAFETY: освобождаем то, что выделила прослойка.
        unsafe { ffi::nonius_mesh_free(&mut m) };
        out
    }

    /// Прямоугольный параллелепипед от угла `origin` с размерами `size`.
    pub fn make_box(origin: [f64; 3], size: [f64; 3]) -> Result<Shape, OcctError> {
        // SAFETY: простые числовые аргументы.
        let ptr = unsafe {
            ffi::nonius_make_box(origin[0], origin[1], origin[2], size[0], size[1], size[2])
        };
        NonNull::new(ptr)
            .map(|ptr| Shape { ptr })
            .ok_or_else(|| OcctError("не удалось построить параллелепипед".into()))
    }

    /// Вертикальный цилиндр: центр основания, радиус, высота.
    pub fn make_cylinder(base: [f64; 3], radius: f64, height: f64) -> Result<Shape, OcctError> {
        // SAFETY: простые числовые аргументы.
        let ptr = unsafe { ffi::nonius_make_cylinder(base[0], base[1], base[2], radius, height) };
        NonNull::new(ptr)
            .map(|ptr| Shape { ptr })
            .ok_or_else(|| OcctError("не удалось построить цилиндр".into()))
    }
}
