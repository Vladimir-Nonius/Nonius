//! Контейнер файла SolidWorks (`.sldprt`, `.sldasm`, `.slddrw`): секции.
//!
//! Новый («блочный») файл SolidWorks — это zip-архив вида OPC, как
//! `.docx`, только спрятанный: перед архивом 8 байт заголовка, вместо
//! сигнатур `PK…` — свои четыре байта, у имён секций в каждом байте
//! переставлены половинки. Секции сжаты deflate без обёртки. Подробно, по
//! байтам — `docs/formats/sldprt-container.md`.
//!
//! Как читаем:
//! 1. Список секций берём из центрального каталога в конце файла — он
//!    перечисляет каждую секцию ровно один раз. Найти его можно только по
//!    записи конца каталога (`layout.rs`), сигнатуры для этого не годятся.
//! 2. Каталога нет (файл обрезан) — запасной путь: идём по меткам
//!    `14 00 06 00 08 00` и верим записи, только если она распаковалась
//!    до заявленного размера и сошлась контрольная сумма. Так отсеиваются
//!    служебные ячейки кэша (они повторяют метку и имя секции, но данных
//!    в них нет) и случайные совпадения внутри сжатых данных. Про такой
//!    файл [`Container::listing`] честно говорит [`Listing::Scan`].
//! 3. Секция распаковывается один раз, при первом обращении, и дальше
//!    отдаётся ссылкой.
//! 4. Распаковка ограничена ([`Budget`]): чужой файл не может заставить
//!    занять память сверх разумного для его размера.
//!
//! Старый контейнер OLE (Compound File) не читается: [`Unreadable::Ole`].
//!
//! Файлы: `layout.rs` — разметка, `budget.rs` — предел распаковки,
//! `pack.rs` — сборка файла для тестов.

mod budget;
mod layout;
mod pack;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::OnceLock;

pub use budget::Budget;
use layout::Slot;

/// Файлы больше не читаем: смещения в контейнере 32-битные.
const MAX_FILE: u64 = u32::MAX as u64;

/// Почему файл не открылся.
#[derive(Debug)]
pub enum Unreadable {
    /// Файл не читается с диска.
    Io(std::io::Error),
    /// Старый контейнер OLE: такие файлы писали прежние версии SolidWorks.
    Ole,
    /// В файле нет ни одной секции SolidWorks.
    Foreign,
    /// Файл больше 4 ГБ.
    Huge,
}

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "файл не читается: {e}"),
            Self::Ole => f.write_str(
                "файл сохранён в старом формате SolidWorks (OLE): прямо из файла \
                 Nonius его не читает, нужен SolidWorks",
            ),
            Self::Foreign => f.write_str("это не файл SolidWorks или он испорчен"),
            Self::Huge => f.write_str("файл больше 4 ГБ — такие Nonius не читает"),
        }
    }
}

impl std::error::Error for Unreadable {}

/// Откуда взят список секций.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listing {
    /// Из центрального каталога: список полный.
    Catalog,
    /// Каталога нет (файл обрезан или испорчен): секции найдены обходом
    /// по меткам, часть могла потеряться.
    Scan,
}

/// Почему секция не отдана.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Не распаковалась, не того размера или не сошлась контрольная сумма.
    Damaged,
    /// Распаковка вышла бы за предел ([`Budget`]).
    OverBudget,
}

/// Ответ на просьбу о секции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section<'a> {
    /// Секции с таким именем в файле нет.
    Missing,
    /// Секция есть, но не читается.
    Refused(Refusal),
    /// Распакованные данные.
    Data(&'a [u8]),
}

struct Entry {
    slot: Slot,
    raw: OnceLock<Result<Vec<u8>, Refusal>>,
}

/// Прочитанный файл SolidWorks.
pub struct Container {
    data: Vec<u8>,
    entries: Vec<Entry>,
    listing: Listing,
    budget: Budget,
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

/// Распаковать deflate без обёртки: ровно в `size` байт, и сжатый поток
/// должен кончиться ровно на границе `packed`.
fn inflate(packed: &[u8], size: usize) -> Option<Vec<u8>> {
    use miniz_oxide::inflate::core::inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
    use miniz_oxide::inflate::core::{decompress, DecompressorOxide};
    use miniz_oxide::inflate::TINFLStatus;
    let mut out = vec![0u8; size.max(1)];
    let mut state = DecompressorOxide::new();
    let flags = TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
    let (status, read, written) = decompress(&mut state, packed, &mut out, 0, flags);
    if status != TINFLStatus::Done || read != packed.len() || written != size {
        return None;
    }
    out.truncate(size);
    Some(out)
}

impl Container {
    /// Прочитать файл с диска.
    pub fn open(path: &Path) -> Result<Self, Unreadable> {
        let size = std::fs::metadata(path).map_err(Unreadable::Io)?.len();
        if size > MAX_FILE {
            return Err(Unreadable::Huge);
        }
        Self::from_bytes(std::fs::read(path).map_err(Unreadable::Io)?)
    }

    /// Контейнер из байтов файла (или вложенного документа: виртуальный
    /// компонент сборки — такой же файл внутри секции).
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, Unreadable> {
        if data.starts_with(&layout::OLE) {
            return Err(Unreadable::Ole);
        }
        if data.len() as u64 > MAX_FILE {
            return Err(Unreadable::Huge);
        }
        let budget = Budget::for_file(data.len());
        let mut file = Self {
            entries: Vec::new(),
            listing: Listing::Catalog,
            budget,
            data,
        };
        match layout::catalog(&file.data) {
            Some(slots) => file.entries = slots.into_iter().map(Entry::new).collect(),
            None => {
                file.listing = Listing::Scan;
                file.scan();
            }
        }
        if file.entries.is_empty() {
            return Err(Unreadable::Foreign);
        }
        // Из одноимённых (в настоящих файлах их нет) отдаём последнюю в
        // файле — порядок секций делаем порядком файла.
        file.entries
            .sort_by_key(|e| e.slot.packed.as_ref().map_or(usize::MAX, |p| p.start));
        Ok(file)
    }

    /// Запасной обход по меткам: принимаем только то, что распаковалось.
    fn scan(&mut self) {
        let mut from = if layout::has_header(&self.data) {
            layout::OUTER
        } else {
            0
        };
        while let Some(at) = layout::next_mark(&self.data, from) {
            from = at + layout::MARK.len();
            let Some((slot, end)) = layout::local(&self.data, at) else {
                continue;
            };
            let Ok(raw) = self.unpack(&slot) else {
                continue;
            };
            let entry = Entry::new(slot);
            let _ = entry.raw.set(Ok(raw));
            self.entries.push(entry);
            // Метка внутри сжатых данных — случайное совпадение.
            from = end;
        }
    }

    fn unpack(&self, slot: &Slot) -> Result<Vec<u8>, Refusal> {
        let packed = slot
            .packed
            .clone()
            .and_then(|range| self.data.get(range))
            .ok_or(Refusal::Damaged)?;
        // Deflate не сжимает сильнее чем в ~1032 раза: больший заявленный
        // размер — порча, память под него не берём.
        let plausible = packed
            .len()
            .checked_mul(1032)
            .and_then(|n| n.checked_add(64));
        if plausible.is_none_or(|most| slot.size > most) {
            return Err(Refusal::Damaged);
        }
        if !self.budget.take(slot.size) {
            return Err(Refusal::OverBudget);
        }
        inflate(packed, slot.size)
            .filter(|raw| crc32(raw) == slot.crc)
            .ok_or(Refusal::Damaged)
    }

    fn read<'a>(&'a self, entry: &'a Entry) -> Section<'a> {
        match entry.raw.get_or_init(|| self.unpack(&entry.slot)) {
            Ok(raw) => Section::Data(raw),
            Err(why) => Section::Refused(*why),
        }
    }

    /// Секция, имя которой подходит под `wanted`. Целая — в первую очередь;
    /// если подходящие есть, но ни одна не читается, — отказ.
    pub fn section_by(&self, wanted: impl Fn(&str) -> bool) -> Section<'_> {
        let mut answer = Section::Missing;
        for entry in self.entries.iter().rev().filter(|e| wanted(&e.slot.name)) {
            match self.read(entry) {
                found @ Section::Data(_) => return found,
                refused => answer = refused,
            }
        }
        answer
    }

    /// Секция по точному имени, например `PreviewPNG`.
    pub fn section(&self, name: &str) -> Section<'_> {
        self.section_by(|n| n == name)
    }

    /// Данные секции, имя которой подходит под `wanted`; `None` — секции
    /// нет или она не читается.
    pub fn chunk_by(&self, wanted: impl Fn(&str) -> bool) -> Option<&[u8]> {
        match self.section_by(wanted) {
            Section::Data(raw) => Some(raw),
            _ => None,
        }
    }

    /// Данные секции по точному имени.
    pub fn chunk(&self, name: &str) -> Option<&[u8]> {
        self.chunk_by(|n| n == name)
    }

    /// Имена секций по алфавиту, без повторов. Ничего не распаковывает.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.entries.iter().map(|e| e.slot.name.clone()).collect();
        names.sort();
        names.dedup();
        names
    }

    /// Сколько байт уже распаковано из этого файла.
    pub fn unpacked(&self) -> u64 {
        self.budget.spent()
    }

    /// Откуда взят список секций: из каталога или обходом обрезанного файла.
    pub fn listing(&self) -> Listing {
        self.listing
    }
}

impl Entry {
    fn new(slot: Slot) -> Self {
        Self {
            slot,
            raw: OnceLock::new(),
        }
    }
}
