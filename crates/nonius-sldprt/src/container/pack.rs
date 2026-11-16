//! Сборка файла-контейнера для тестов.
//!
//! Настоящие файлы людей в репозиторий не кладём, поэтому тесты собирают
//! файл сами. Раскладка повторяет настоящую (`sldprt-container.md` §2):
//! заголовок, служебная ячейка кэша, записи секций, каталог, сетка между
//! каталогом и записью конца каталога, хвост после неё. Непрозрачные байты
//! (сигнатуры, заполнение сеток) здесь произвольные: SolidWorks такой файл
//! не откроет, он нужен только нашему читателю.

use super::layout::{swap_halves, MARK, OUTER};
use super::{crc32, Container};

const LOCAL_SIGN: [u8; 4] = [0xa1, 0xb2, 0xc3, 0xd4];
const CENTRAL_SIGN: [u8; 4] = [0xa5, 0xb6, 0xc7, 0xd8];
const END_SIGN: [u8; 4] = [0xa9, 0xba, 0xcb, 0xdc];

fn u16le(n: usize) -> [u8; 2] {
    u16::try_from(n).unwrap_or(u16::MAX).to_le_bytes()
}

fn u32le(n: usize) -> [u8; 4] {
    u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes()
}

fn hidden(name: &str) -> impl Iterator<Item = u8> + '_ {
    name.bytes().map(swap_halves)
}

/// Ячейка кэша: метка и имя секции, но без данных. Похожа на запись.
fn cache_cell(name: &str) -> Vec<u8> {
    let size = 5 * 90;
    let mut cell = vec![0x77; 5];
    cell.extend(MARK);
    cell.extend([0; 4]);
    cell.extend(u32le(2 * size));
    cell.extend(u32le(size / 2));
    cell.extend(u32le(size));
    cell.extend(u32le(name.len()));
    cell.extend(hidden(name));
    cell.resize(size, 0x77);
    cell
}

impl Container {
    /// Собрать файл из секций `(имя, данные)`.
    pub fn pack(chunks: &[(&str, &[u8])]) -> Vec<u8> {
        Self::pack_with(chunks, &[])
    }

    /// То же; у каждой записи после имени лежит дополнительное поле `extra`
    /// (в настоящих файлах оно пустое, но формат его разрешает).
    pub fn pack_with(chunks: &[(&str, &[u8])], extra: &[u8]) -> Vec<u8> {
        let mut file = vec![0x1b, 0x13, 0xa4, 0xb4, 0, 0, 0, 4];
        if let Some((name, _)) = chunks.first() {
            file.extend(cache_cell(name));
        }
        let mut central = Vec::new();
        for (name, raw) in chunks {
            let packed = miniz_oxide::deflate::compress_to_vec(raw, 6);
            let mut head = MARK.to_vec();
            head.extend([0; 4]);
            head.extend(crc32(raw).to_le_bytes());
            head.extend(u32le(packed.len()));
            head.extend(u32le(raw.len()));
            head.extend(u16le(name.len()));
            head.extend(u16le(extra.len()));

            central.extend(CENTRAL_SIGN);
            central.extend([0; 2]);
            central.extend(&head);
            central.extend([0; 10]); // comment, disk, attributes
            central.extend(u32le(file.len() - OUTER));
            central.extend(hidden(name));
            central.extend(extra);

            file.extend(LOCAL_SIGN);
            file.extend(head);
            file.extend(hidden(name));
            file.extend(extra);
            file.extend(packed);
        }
        let first = file.len();
        file.extend(central);
        file.extend([0x55; 39]); // сетка между каталогом и концом каталога
        let end = file.len();
        file.extend(END_SIGN);
        file.extend([0; 4]);
        file.extend(u16le(chunks.len()));
        file.extend(u16le(chunks.len()));
        file.extend(u32le(end - first));
        file.extend(u32le(first - OUTER));
        file.extend([0; 2]);
        // Хвост: ячейка, похожая на конец каталога (те же счётчики).
        file.extend([0x66; 9]);
        file.extend([0; 4]);
        file.extend(u16le(chunks.len()));
        file.extend(u16le(chunks.len()));
        file.extend(u32le(90));
        file.extend(u32le(5 * 7 * 85));
        file.extend([0; 2]);
        file.extend([0x66; 30]);
        file
    }
}
