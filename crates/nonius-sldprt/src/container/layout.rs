//! Разметка контейнера: каталог секций, запись конца каталога, запасной
//! обход по меткам. Описание байтов — `docs/formats/sldprt-container.md` §2.
//!
//! Здесь только адреса и размеры: ничего не распаковывается. Вся
//! арифметика смещений — с проверкой переполнения.

use std::ops::Range;

/// Метка записи: version 20, flags 6, method 8 (deflate).
pub(super) const MARK: [u8; 6] = [0x14, 0x00, 0x06, 0x00, 0x08, 0x00];
/// Внешний заголовок: 4 байта номера файла и слово версии.
pub(super) const OUTER: usize = 8;
/// Старый контейнер OLE2 (Compound File Binary).
pub(super) const OLE: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
/// Локальная запись от метки до имени.
const LOCAL: usize = 26;
/// Запись каталога до имени.
const CENTRAL: usize = 46;
/// Запись конца каталога.
const END: usize = 22;
/// Имя длиннее — это не запись, а совпадение в данных.
const MAX_NAME: usize = 500;

/// Секция, как она описана в файле: где лежат сжатые данные и что должно
/// получиться после распаковки.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Slot {
    pub name: String,
    pub crc: u32,
    /// Сжатые данные. `None` — каталог называет секцию, но её записи на
    /// месте нет (файл испорчен).
    pub packed: Option<Range<usize>>,
    pub size: usize,
}

fn u16_at(data: &[u8], at: usize) -> Option<usize> {
    let bytes = data.get(at..at.checked_add(2)?)?;
    Some(usize::from(u16::from_le_bytes(bytes.try_into().ok()?)))
}

fn u32_at(data: &[u8], at: usize) -> Option<usize> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    usize::try_from(u32::from_le_bytes(bytes.try_into().ok()?)).ok()
}

fn crc_at(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

/// Переставить половинки байта: так в файле спрятаны имена секций.
pub(super) fn swap_halves(byte: u8) -> u8 {
    byte.rotate_left(4)
}

/// Имя секции из спрятанных байтов. Имена — UTF-8, бывают русские.
fn name(hidden: &[u8]) -> Option<String> {
    let plain: Vec<u8> = hidden.iter().map(|b| swap_halves(*b)).collect();
    if plain.is_empty() || plain.len() > MAX_NAME || plain.iter().any(|b| *b < 0x20) {
        return None;
    }
    Some(String::from_utf8_lossy(&plain).into_owned())
}

/// Слово версии во внешнем заголовке (big-endian) равно 4.
pub(super) fn has_header(data: &[u8]) -> bool {
    data.get(4..OUTER) == Some(&[0, 0, 0, 4])
}

/// Локальная запись, метка которой стоит по адресу `at`: секция и адрес
/// первого байта после её данных.
pub(super) fn local(data: &[u8], at: usize) -> Option<(Slot, usize)> {
    if data.get(at..at.checked_add(MARK.len())?)? != MARK {
        return None;
    }
    let packed = u32_at(data, at + 14)?;
    let name_at = at.checked_add(LOCAL)?;
    let extra_at = name_at.checked_add(u16_at(data, at + 22)?)?;
    let start = extra_at.checked_add(u16_at(data, at + 24)?)?;
    let end = start.checked_add(packed)?;
    data.get(start..end)?;
    let slot = Slot {
        name: name(data.get(name_at..extra_at)?)?,
        crc: crc_at(data, at + 10)?,
        packed: Some(start..end),
        size: u32_at(data, at + 18)?,
    };
    Some((slot, end))
}

/// Секции по центральному каталогу. `None` — записи конца каталога нет
/// (файл обрезан) или каталог не сходится сам с собой.
pub(super) fn catalog(data: &[u8]) -> Option<Vec<Slot>> {
    let last = data.len().checked_sub(END)?;
    (OUTER..=last).rev().find_map(|at| listed(data, at))
}

/// Запись конца каталога по адресу `end` — настоящая, если сходятся оба
/// уравнения: каталог начинается там, куда она показывает, и кончается
/// ровно на ней. Хвостовые ячейки файла похожи на неё, но уравнений не
/// выдерживают.
fn listed(data: &[u8], end: usize) -> Option<Vec<Slot>> {
    if crc_at(data, end + 4)? != 0 || u16_at(data, end + 20)? != 0 {
        return None;
    }
    let count = u16_at(data, end + 8)?;
    if count == 0 || count != u16_at(data, end + 10)? {
        return None;
    }
    let first = u32_at(data, end + 16)?.checked_add(OUTER)?;
    if first.checked_add(u32_at(data, end + 12)?)? != end {
        return None;
    }
    let mut slots = Vec::new();
    let mut at = first;
    for _ in 0..count {
        let (slot, next) = central(data, at, end)?;
        slots.push(slot);
        at = next;
    }
    Some(slots)
}

/// Запись каталога по адресу `at` и адрес следующей.
fn central(data: &[u8], at: usize, limit: usize) -> Option<(Slot, usize)> {
    let same = data.get(at.checked_add(6)?..at.checked_add(6 + LOCAL)?)?;
    if same[..MARK.len()] != MARK {
        return None;
    }
    let name_at = at.checked_add(CENTRAL)?;
    let name_end = name_at.checked_add(u16_at(data, at + 28)?)?;
    let next = name_end
        .checked_add(u16_at(data, at + 30)?)?
        .checked_add(u16_at(data, at + 32)?)?;
    if next > limit {
        return None;
    }
    let mut slot = Slot {
        name: name(data.get(name_at..name_end)?)?,
        crc: crc_at(data, at + 16)?,
        packed: None,
        size: u32_at(data, at + 24)?,
    };
    // Локальная запись повторяет 26 байт каталога; не повторяет — секции
    // на месте нет, и читать по этому адресу нельзя.
    let mark = u32_at(data, at + 42)?
        .checked_add(OUTER)
        .and_then(|o| o.checked_add(4));
    if let Some(mark) = mark {
        let there = mark.checked_add(LOCAL).and_then(|e| data.get(mark..e));
        if there == Some(same) {
            slot.packed = local(data, mark).and_then(|(found, _)| found.packed);
        }
    }
    Some((slot, next))
}

/// Запасной обход для файла без каталога: все места, где стоит метка,
/// начиная с `from`. Какие из них — настоящие записи, решает вызывающий,
/// распаковав данные: метка встречается и в служебных ячейках, и внутри
/// сжатых данных.
pub(super) fn next_mark(data: &[u8], from: usize) -> Option<usize> {
    data.get(from..)?
        .windows(MARK.len())
        .position(|w| w == MARK)
        .and_then(|p| p.checked_add(from))
}
