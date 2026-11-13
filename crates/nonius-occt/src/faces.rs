//! Изменённые грани двух версий: какие грани есть только в одной из них.
//!
//! Это быстро — доли секунды даже на детали, где булева операция идёт
//! секунды: грани только сопоставляются (`cpp/match.cpp`), ничего не
//! пересекается. По описаниям граней пишутся слова ([`crate::changes`]),
//! сами грани можно показать раньше, чем посчитаны объёмы.

use std::ptr::NonNull;

use crate::changes::{Face, Side, Surface};
use crate::ffi;
use crate::shape::Shape;

/// Грани, которые не совпали у двух версий.
pub struct ChangedFaces {
    /// Описания граней: сначала пропавшие, потом появившиеся.
    pub faces: Vec<Face>,
    /// Появившиеся грани — из «стало».
    pub fresh: Shape,
    /// Пропавшие грани — из «было».
    pub gone: Shape,
}

fn face(f: &ffi::NoniusFace) -> Face {
    Face {
        side: if f.side == 0 {
            Side::Before
        } else {
            Side::After
        },
        surface: match f.kind {
            1 => Surface::Plane,
            2 => Surface::Cylinder,
            3 => Surface::Cone,
            4 => Surface::Sphere,
            5 => Surface::Torus,
            _ => Surface::Other,
        },
        inward: f.inward == 1,
        origin: f.origin,
        dir: f.dir,
        radius: f.radius,
        radius2: f.radius2,
        angle: f.angle,
        span: (f.from, f.to),
        area: f.area,
        center: f.center,
    }
}

impl Shape {
    /// Грани, которые есть только в `self` («было») или только в `after`.
    /// `None` — версии устроены слишком по-разному, сопоставить их нельзя.
    pub fn changed_faces(&self, after: &Shape) -> Option<ChangedFaces> {
        let mut list: *mut ffi::NoniusFace = std::ptr::null_mut();
        let mut count = 0usize;
        let mut fresh: *mut ffi::NoniusShape = std::ptr::null_mut();
        let mut gone: *mut ffi::NoniusShape = std::ptr::null_mut();
        // SAFETY: оба тела живые, указатели на результаты действительны.
        let ok = unsafe {
            ffi::nonius_changed_faces(
                self.ptr.as_ptr(),
                after.ptr.as_ptr(),
                &mut list,
                &mut count,
                &mut fresh,
                &mut gone,
            )
        };
        if ok != 1 {
            return None;
        }
        let faces = if list.is_null() {
            Vec::new()
        } else {
            // SAFETY: прослойка выделила ровно `count` описаний.
            unsafe { std::slice::from_raw_parts(list, count) }
                .iter()
                .map(face)
                .collect()
        };
        // SAFETY: список выделен прослойкой и освобождается один раз; нулевой — можно.
        unsafe { ffi::nonius_faces_free(list) };
        // Оба тела оборачиваются сразу, чтобы при отказе одно не утекло.
        let fresh = NonNull::new(fresh).map(Shape::own);
        let gone = NonNull::new(gone).map(Shape::own);
        Some(ChangedFaces {
            faces,
            fresh: fresh?,
            gone: gone?,
        })
    }
}
