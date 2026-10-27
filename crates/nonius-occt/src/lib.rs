//! Геометрия Nonius на OpenCascade.
//!
//! - [`OcctStepSource`] — читает STEP: объём, площадь, центр масс, габариты;
//! - [`GeometryComparator`] — сравнивает форму: сколько материала добавлено и убрано;
//! - [`HighlightRenderer`] — рисует изменения цветом: неизменённое серым
//!   «рентгеном», добавленное зелёным, убранное красным;
//! - [`PreviewExporter`] — превью детали `preview.png` (экспорт `png`);
//! - [`render`] — свой растровый рендер, без видеокарты и графических библиотек.
//!
//! OpenCascade подключается через тонкую C-прослойку (`cpp/`). Если при сборке
//! OpenCascade не найден, крейт собирается без геометрии: [`available`] вернёт
//! `false`, а [`register`] ничего не подключит — программа работает дальше.

#[cfg(nonius_occt)]
mod ffi;
#[cfg(nonius_occt)]
mod modules;
pub mod render;
#[cfg(nonius_occt)]
mod shape;

use std::fmt;
use std::fs::File;
use std::io::{self, BufWriter};
use std::path::Path;

use nonius_core::extension::Registry;

#[cfg(nonius_occt)]
pub use modules::{
    preview, GeometryComparator, GeometryDiff, HighlightRenderer, OcctStepSource, PreviewExporter,
    CHANGES_PNG, MODEL_STEP, PREVIEW_PNG,
};
#[cfg(nonius_occt)]
pub use shape::{Props, Shape};

/// Ошибка OpenCascade или прослойки.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcctError(pub String);

impl fmt::Display for OcctError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OcctError {}

/// Собрана ли программа с OpenCascade.
pub fn available() -> bool {
    cfg!(nonius_occt)
}

/// Версия OpenCascade, если он подключён.
pub fn occt_version() -> Option<String> {
    #[cfg(nonius_occt)]
    {
        Some(shape::version())
    }
    #[cfg(not(nonius_occt))]
    {
        None
    }
}

/// Подключить модули геометрии к реестру. Источник STEP встаёт первым,
/// чтобы заменить чтение одного заголовка.
pub fn register(registry: &mut Registry) -> nonius_core::Result<()> {
    #[cfg(nonius_occt)]
    {
        registry.add_geometry_source(Box::new(OcctStepSource::new()))?;
        registry.add_comparator(Box::new(GeometryComparator::new()))?;
        registry.add_renderer(Box::new(HighlightRenderer::new()))?;
        registry.add_exporter(Box::new(PreviewExporter::new()))?;
    }
    #[cfg(not(nonius_occt))]
    {
        let _ = registry;
    }
    Ok(())
}

/// Нарисовать превью детали из STEP в PNG. `Ok(false)` — программа собрана
/// без OpenCascade, превью не сделано.
pub fn preview_png(step: &Path, png_path: &Path) -> Result<bool, OcctError> {
    #[cfg(nonius_occt)]
    {
        let view = render::View {
            width: 480,
            height: 360,
            ..render::View::default()
        };
        let img = preview(step, &view)?;
        save_png(&img, png_path).map_err(|e| OcctError(format!("{}: {e}", png_path.display())))?;
        Ok(true)
    }
    #[cfg(not(nonius_occt))]
    {
        let _ = (step, png_path);
        Ok(false)
    }
}

/// Сохранить картинку в PNG.
pub fn save_png(img: &render::Image, path: &Path) -> io::Result<()> {
    let file = File::create(path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), img.width, img.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(io::Error::other)?;
    writer
        .write_image_data(&img.rgba)
        .map_err(io::Error::other)?;
    Ok(())
}
