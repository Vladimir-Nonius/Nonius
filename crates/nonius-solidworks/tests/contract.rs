//! Контракт модуля `solidworks-step`.
//!
//! Сама выгрузка запускает настоящий SolidWorks, поэтому идёт только по
//! просьбе: задать `NONIUS_TEST_SOLIDWORKS` — путь к любой детали `.sldprt`.
//! Без этого проверяется то, что можно проверить без SolidWorks.

use std::path::Path;

use nonius_core::contract;
use nonius_core::extension::{Converter, ExtensionPoint};
use nonius_solidworks::SolidWorksConverter;

#[test]
fn manifest_and_file_kinds() {
    let c = SolidWorksConverter::new();
    if let Err(v) = contract::manifest(&c, ExtensionPoint::Converter) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
    assert!(c.can_convert(Path::new("Вал.SLDPRT")));
    assert!(c.can_convert(Path::new("узел.sldasm")));
    assert!(!c.can_convert(Path::new("вал.step")));
    assert!(!c.can_convert(Path::new("чертёж.slddrw")));
}

#[test]
fn missing_file_is_an_error_in_words() {
    let dir = tempfile::tempdir().unwrap();
    let err = SolidWorksConverter::new()
        .to_step(&dir.path().join("нет.sldprt"), &dir.path().join("нет.step"))
        .unwrap_err();
    assert!(err.to_string().contains("файл не найден"), "{err}");
    assert!(!dir.path().join("нет.step").exists());
}

#[test]
fn real_solidworks_passes_contract() {
    let Some(sample) = std::env::var_os("NONIUS_TEST_SOLIDWORKS") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let c = SolidWorksConverter::new();
    if let Err(v) = contract::converter(&c, Path::new(&sample), dir.path()) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
}
