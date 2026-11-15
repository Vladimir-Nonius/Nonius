//! Контракт модуля `solidworks-step`.
//!
//! Сама выгрузка запускает настоящий SolidWorks, поэтому идёт только по
//! просьбе: задать `NONIUS_TEST_SOLIDWORKS` — путь к любой детали `.sldprt`.
//! Без этого проверяется то, что можно проверить без SolidWorks.

use std::path::Path;

use nonius_core::contract;
use nonius_core::extension::{Converter, ExtensionPoint};
use nonius_solidworks::{SolidWorksConverter, SolidWorksLinks};

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
    if let Err(v) = contract::link_source(&SolidWorksLinks::new(), Path::new(&sample)) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
}

/// Номера процессов SolidWorks на этом компьютере.
fn solidworks_processes() -> Vec<u32> {
    let listed = std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq SLDWORKS.exe", "/FO", "CSV", "/NH"])
        .output();
    let text = listed.map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
    text.unwrap_or_default()
        .lines()
        .filter_map(|l| l.split(',').nth(1)?.trim_matches('"').parse().ok())
        .collect()
}

/// Свой экземпляр: один на все просьбы, не тот, что открыт человеком, и
/// SolidWorks человека после работы на месте. Путь — с кириллицей и
/// пробелом. Чтобы проверить «человека», откройте SolidWorks перед тестом.
#[test]
fn real_solidworks_is_our_own_instance() {
    let Some(sample) = std::env::var_os("NONIUS_TEST_SOLIDWORKS") else {
        return;
    };
    let before = solidworks_processes();
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("КТМ 640");
    std::fs::create_dir(&folder).unwrap();
    let part = folder.join("вал копия.SLDPRT");
    std::fs::copy(&sample, &part).unwrap();

    let own = nonius_solidworks::ping().unwrap();
    assert!(own.pid != 0 && own.started != 0, "{own:?}");
    assert!(own.fenced, "свой SolidWorks должен быть в группе процессов");
    assert!(
        !before.contains(&own.pid),
        "это SolidWorks человека: {own:?}, были {before:?}"
    );
    assert_eq!(nonius_solidworks::status(), nonius_solidworks::Status::Idle);

    let c = SolidWorksConverter::new();
    assert!(c.stamp(&part).is_some(), "условия перевода известны");
    let step = folder.join("вал копия.step");
    c.to_step(&part, &step).unwrap();
    assert!(std::fs::metadata(&step).unwrap().len() > 100);
    use nonius_core::extension::LinkSource;
    SolidWorksLinks::new()
        .links(std::slice::from_ref(&part))
        .unwrap();
    // Все просьбы обслужил один и тот же экземпляр.
    assert_eq!(nonius_solidworks::own(), Some(own.clone()));

    // SolidWorks человека никто не закрыл, а наш к ним не прибавился дважды.
    let after = solidworks_processes();
    for human in &before {
        assert!(after.contains(human), "закрыт SolidWorks человека {human}");
    }
    assert_eq!(after.len(), before.len() + 1, "{before:?} → {after:?}");
    println!(
        "свой SolidWorks: процесс {}, версия {}; у человека: {before:?}",
        own.pid, own.revision
    );
}

#[test]
fn links_manifest_and_file_kinds() {
    let l = SolidWorksLinks::new();
    if let Err(v) = contract::manifest(&l, ExtensionPoint::Links) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
    use nonius_core::extension::LinkSource;
    assert!(l.knows(Path::new("узел.SLDASM")));
    assert!(l.knows(Path::new("вал.slddrw")));
    assert!(!l.knows(Path::new("вал.step")));
    assert_eq!(l.links(&[]).unwrap(), Vec::<Vec<std::path::PathBuf>>::new());
}
