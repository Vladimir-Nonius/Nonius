//! Тесты контейнера: файл собирается здесь же (`pack.rs`).

use super::*;

const PARTS: [(&str, &[u8]); 3] = [
    ("PreviewPNG", b"picture picture picture"),
    ("VirtualComp/Деталь3", b"nested"),
    ("ThirdPty/Empty", b""),
];

fn parts() -> Vec<(&'static str, &'static [u8])> {
    PARTS.to_vec()
}

#[test]
fn sections_come_from_the_catalog_and_unpack_once() {
    let file = Container::from_bytes(Container::pack(&parts())).unwrap();
    assert_eq!(file.listing(), Listing::Catalog);
    assert_eq!(
        file.names(),
        ["PreviewPNG", "ThirdPty/Empty", "VirtualComp/Деталь3"]
    );
    // Список секций ничего не распаковал; ячейка кэша секцией не стала.
    assert_eq!(file.unpacked(), 0);
    assert_eq!(file.chunk("PreviewPNG"), Some(PARTS[0].1));
    assert_eq!(file.chunk("PreviewPNG"), Some(PARTS[0].1));
    assert_eq!(file.unpacked(), PARTS[0].1.len() as u64);
    assert_eq!(file.chunk("VirtualComp/Деталь3"), Some(&b"nested"[..]));
    assert_eq!(file.chunk("ThirdPty/Empty"), Some(&b""[..]));
    assert_eq!(file.section("нет такой"), Section::Missing);
    assert_eq!(
        file.chunk_by(|n| n.starts_with("ThirdPty/")),
        Some(&b""[..])
    );
}

#[test]
fn extra_field_after_the_name_is_skipped() {
    let file = Container::from_bytes(Container::pack_with(&parts(), b"extra!")).unwrap();
    assert_eq!(file.listing(), Listing::Catalog);
    assert_eq!(file.chunk("PreviewPNG"), Some(PARTS[0].1));
}

#[test]
fn cut_file_is_read_by_scanning() {
    let whole = Container::pack(&parts());
    // Без хвоста и конца каталога: секции целы, каталога нет.
    let cut = Container::from_bytes(whole[..whole.len() - 60].to_vec()).unwrap();
    assert_eq!(cut.listing(), Listing::Scan);
    assert_eq!(cut.names().len(), 3);
    assert_eq!(cut.chunk("PreviewPNG"), Some(PARTS[0].1));
    // Обрезан посреди секций: что уцелело, то и отдаём.
    let at = whole
        .windows(4)
        .rposition(|w| w == [0xa1, 0xb2, 0xc3, 0xd4]);
    let half = Container::from_bytes(whole[..at.unwrap() + 20].to_vec()).unwrap();
    assert_eq!(half.listing(), Listing::Scan);
    assert_eq!(half.names(), ["PreviewPNG", "VirtualComp/Деталь3"]);
}

#[test]
fn damaged_section_is_refused_and_the_rest_is_read() {
    let mut bytes = Container::pack(&parts());
    let at = bytes
        .windows(4)
        .position(|w| w == [0xa1, 0xb2, 0xc3, 0xd4])
        .unwrap();
    // Портим контрольную сумму в первой записи, а каталог не трогаем:
    // запись перестаёт повторять каталог.
    bytes[at + 14] ^= 0xff;
    let file = Container::from_bytes(bytes).unwrap();
    assert_eq!(file.names().len(), 3);
    assert_eq!(
        file.section("PreviewPNG"),
        Section::Refused(Refusal::Damaged)
    );
    assert_eq!(file.chunk("VirtualComp/Деталь3"), Some(&b"nested"[..]));
}

#[test]
fn size_lie_does_not_take_memory() {
    let mut bytes = Container::pack(&parts());
    let local = bytes
        .windows(4)
        .position(|w| w == [0xa1, 0xb2, 0xc3, 0xd4])
        .unwrap();
    let central = bytes
        .windows(4)
        .position(|w| w == [0xa5, 0xb6, 0xc7, 0xd8])
        .unwrap();
    // Заявленный размер — 4 ГиБ без байта, в записи и в каталоге.
    bytes[local + 22..local + 26].fill(0xff);
    bytes[central + 24..central + 28].fill(0xff);
    let file = Container::from_bytes(bytes).unwrap();
    assert_eq!(
        file.section("PreviewPNG"),
        Section::Refused(Refusal::Damaged)
    );
    assert_eq!(file.unpacked(), 0);
}

#[test]
fn old_and_foreign_files_are_named() {
    let mut ole = vec![0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
    ole.resize(2048, 0);
    let old = Container::from_bytes(ole).err().unwrap();
    assert!(matches!(old, Unreadable::Ole));
    assert!(old.to_string().contains("старом формате"), "{old}");

    for junk in [
        &b""[..],
        b"\x14\x00\x06\x00\x08\x00 not a part",
        &[0u8; 4096],
    ] {
        let why = Container::from_bytes(junk.to_vec()).err().unwrap();
        assert!(matches!(why, Unreadable::Foreign), "{why}");
    }
    let gone = Container::open(Path::new("нет такого файла.sldprt"));
    assert!(matches!(gone.err().unwrap(), Unreadable::Io(_)));
}
