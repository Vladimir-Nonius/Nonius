//! Контейнер файла SolidWorks (2015 и новее): список сжатых кусков.
//!
//! Файл `.sldprt` — не OLE и не zip, но устроен похоже на zip: подряд идут
//! куски, у каждого заголовок и сжатые данные (deflate без обёртки).
//!
//! Заголовок куска, числа little-endian:
//!
//! | байты | что |
//! |---|---|
//! | 6 | метка `14 00 06 00 08 00` |
//! | 4 | время записи (не нужно) |
//! | 4 | CRC-32 распакованных данных |
//! | 4 | размер сжатых данных |
//! | 4 | размер распакованных данных |
//! | 4 | длина имени |
//! | … | имя: у каждого байта половинки переставлены местами |
//! | … | сжатые данные |
//!
//! В начале файла лежат обрывки старых заголовков с теми же именами, а
//! метка может случайно встретиться внутри сжатых данных. Поэтому куску
//! верим, только когда он распаковался до заявленного размера и сошлась
//! контрольная сумма; из одноимённых берём последний.

use std::path::Path;

const MARK: [u8; 6] = [0x14, 0x00, 0x06, 0x00, 0x08, 0x00];
const HEADER: usize = 26;
/// Имя длиннее — это не заголовок, а совпадение в данных.
const MAX_NAME: usize = 300;
/// Больше куска не распаковываем: защита от испорченного размера.
const MAX_CHUNK: usize = 1 << 30;

/// Прочитанный файл SolidWorks.
pub struct Container {
    data: Vec<u8>,
}

struct Entry {
    name: String,
    crc: u32,
    packed: std::ops::Range<usize>,
    size: usize,
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

/// Переставить половинки байта: так в файле спрятаны имена кусков.
pub(crate) fn swap_halves(byte: u8) -> u8 {
    byte.rotate_left(4)
}

/// CRC-32, как в zip.
pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

impl Container {
    /// Прочитать файл целиком.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        Ok(Self::from_bytes(std::fs::read(path)?))
    }

    /// Контейнер из байтов файла.
    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Собрать файл из кусков `(имя, данные)` — тем же способом, каким
    /// его пишет SolidWorks. Нужно тестам: настоящие файлы в репозиторий
    /// не кладём.
    pub fn pack(chunks: &[(&str, &[u8])]) -> Vec<u8> {
        let mut file = vec![0x1b, 0x13, 0xa4, 0xb4];
        let number = |n: usize| u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes();
        for (name, raw) in chunks {
            let packed = miniz_oxide::deflate::compress_to_vec(raw, 6);
            file.extend(MARK);
            file.extend([0; 4]);
            file.extend(crc32(raw).to_le_bytes());
            file.extend(number(packed.len()));
            file.extend(number(raw.len()));
            file.extend(number(name.len()));
            file.extend(name.bytes().map(swap_halves));
            file.extend(packed);
        }
        file
    }

    /// Заголовок куска по адресу `at`, если он там похож на настоящий.
    fn entry(&self, at: usize) -> Option<Entry> {
        let crc = u32_at(&self.data, at + 10)?;
        let packed = u32_at(&self.data, at + 14)? as usize;
        let size = u32_at(&self.data, at + 18)? as usize;
        let name_len = u32_at(&self.data, at + 22)? as usize;
        if name_len == 0 || name_len > MAX_NAME || size > MAX_CHUNK {
            return None;
        }
        let start = at + HEADER + name_len;
        let raw = self.data.get(at + HEADER..start)?;
        self.data.get(start..start + packed)?;
        let name: Vec<u8> = raw.iter().map(|b| swap_halves(*b)).collect();
        if name.iter().any(|b| *b < 0x20) {
            return None;
        }
        Some(Entry {
            name: String::from_utf8_lossy(&name).into_owned(),
            crc,
            packed: start..start + packed,
            size,
        })
    }

    /// Все заголовки подряд — и настоящие, и обрывки.
    fn entries(&self) -> Vec<Entry> {
        let mut found = Vec::new();
        let mut from = 0;
        while let Some(at) = self.data[from..]
            .windows(MARK.len())
            .position(|w| w == MARK)
            .map(|p| p + from)
        {
            from = at + MARK.len();
            if let Some(entry) = self.entry(at) {
                found.push(entry);
            }
        }
        found
    }

    fn unpack(&self, entry: &Entry) -> Option<Vec<u8>> {
        let packed = &self.data[entry.packed.clone()];
        let raw = miniz_oxide::inflate::decompress_to_vec_with_limit(packed, entry.size).ok()?;
        (raw.len() == entry.size && crc32(&raw) == entry.crc).then_some(raw)
    }

    /// Распакованный кусок, имя которого подходит под `wanted`. Из
    /// нескольких — последний целый.
    pub fn chunk_by(&self, wanted: impl Fn(&str) -> bool) -> Option<Vec<u8>> {
        self.entries()
            .iter()
            .rev()
            .filter(|e| wanted(&e.name))
            .find_map(|e| self.unpack(e))
    }

    /// Распакованный кусок по точному имени, например `PreviewPNG`.
    pub fn chunk(&self, name: &str) -> Option<Vec<u8>> {
        self.chunk_by(|n| n == name)
    }

    /// Имена целых кусков, по алфавиту и без повторов.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .entries()
            .iter()
            .filter(|e| self.unpack(e).is_some())
            .map(|e| e.name.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }
}
