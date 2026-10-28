//! Объявления C-прослойки (`cpp/nonius_occt.h`).

use std::os::raw::c_char;

#[repr(C)]
pub(crate) struct NoniusShape {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Default)]
pub(crate) struct NoniusProps {
    pub volume: f64,
    pub area: f64,
    pub center: [f64; 3],
    pub has_bbox: i32,
    pub bbox_min: [f64; 3],
    pub bbox_max: [f64; 3],
    pub solids: u32,
    pub faces: u32,
}

#[repr(C)]
pub(crate) struct NoniusMesh {
    pub positions: *mut f32,
    pub vertex_count: usize,
    pub indices: *mut u32,
    pub triangle_count: usize,
}

unsafe extern "C" {
    pub(crate) fn nonius_occt_version() -> *const c_char;
    pub(crate) fn nonius_read_step(
        path: *const c_char,
        err: *mut c_char,
        err_len: usize,
    ) -> *mut NoniusShape;
    pub(crate) fn nonius_write_step(
        shape: *const NoniusShape,
        path: *const c_char,
        err: *mut c_char,
        err_len: usize,
    ) -> i32;
    pub(crate) fn nonius_shape_free(shape: *mut NoniusShape);
    pub(crate) fn nonius_shape_solid(shape: *const NoniusShape, index: u32) -> *mut NoniusShape;
    pub(crate) fn nonius_write_brep(shape: *const NoniusShape, path: *const c_char) -> i32;
    pub(crate) fn nonius_read_brep(path: *const c_char) -> *mut NoniusShape;
    pub(crate) fn nonius_shape_props(shape: *const NoniusShape, out: *mut NoniusProps) -> i32;
    pub(crate) fn nonius_shape_volume(shape: *const NoniusShape) -> f64;
    pub(crate) fn nonius_shape_solid_count(shape: *const NoniusShape) -> u32;
    pub(crate) fn nonius_shape_rough_bbox(
        shape: *const NoniusShape,
        min: *mut f64,
        max: *mut f64,
    ) -> i32;
    pub(crate) fn nonius_boolean(
        a: *const NoniusShape,
        b: *const NoniusShape,
        op: i32,
        fuzzy: f64,
        err: *mut c_char,
        err_len: usize,
    ) -> *mut NoniusShape;
    pub(crate) fn nonius_diff(
        before: *const NoniusShape,
        after: *const NoniusShape,
        added: *mut *mut NoniusShape,
        removed: *mut *mut NoniusShape,
        err: *mut c_char,
        err_len: usize,
    ) -> i32;
    pub(crate) fn nonius_shape_mesh(
        shape: *const NoniusShape,
        linear_deflection: f64,
        angular_deflection: f64,
        out: *mut NoniusMesh,
    ) -> i32;
    pub(crate) fn nonius_mesh_free(mesh: *mut NoniusMesh);
    pub(crate) fn nonius_make_box(
        x: f64,
        y: f64,
        z: f64,
        dx: f64,
        dy: f64,
        dz: f64,
    ) -> *mut NoniusShape;
    pub(crate) fn nonius_make_cylinder(
        x: f64,
        y: f64,
        z: f64,
        radius: f64,
        height: f64,
    ) -> *mut NoniusShape;
}
