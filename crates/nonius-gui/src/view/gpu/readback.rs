//! Чтение готового кадра с видеокарты: для «Сохранить картинку» и тестов.

use std::path::Path;

use super::Gpu;

/// Картинка RGBA, строки сверху вниз.
#[derive(Debug, Clone)]
pub struct Image {
    /// Ширина, пикселей.
    pub width: u32,
    /// Высота, пикселей.
    pub height: u32,
    /// Пиксели по 4 байта.
    pub rgba: Vec<u8>,
}

impl Image {
    /// Пиксель (x, y).
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    /// Сохранить в PNG.
    pub fn save_png(&self, path: &Path) -> anyhow::Result<()> {
        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut enc = png::Encoder::new(file, self.width, self.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()?.write_image_data(&self.rgba)?;
        Ok(())
    }
}

impl Gpu {
    /// Прочитать последний нарисованный кадр. `None` — кадра ещё не было.
    pub fn read_image(&self) -> anyhow::Result<Option<Image>> {
        let Some(t) = &self.target else {
            return Ok(None);
        };
        let [width, height] = t.size;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let row = (width * 4).div_ceil(align) * align;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            t.color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([enc.finish()]);

        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::Wait)
            .map_err(|e| anyhow::anyhow!("видеокарта не ответила: {e}"))?;
        rx.recv()?
            .map_err(|e| anyhow::anyhow!("не удалось прочитать кадр: {e}"))?;
        let data = slice.get_mapped_range();
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height as usize {
            let start = y * row as usize;
            rgba.extend_from_slice(&data[start..start + width as usize * 4]);
        }
        drop(data);
        buffer.unmap();
        Ok(Some(Image {
            width,
            height,
            rgba,
        }))
    }
}
