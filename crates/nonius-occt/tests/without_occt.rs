//! Поведение без OpenCascade: программа работает, геометрия просто отключена.

use nonius_core::extension::Registry;

#[test]
fn register_never_breaks_registry() {
    let mut r = Registry::with_builtins();
    nonius_occt::register(&mut r).unwrap();
    let ids: Vec<_> = r.manifests().iter().map(|m| m.id.clone()).collect();
    assert!(ids.contains(&"meta-compare".to_string()));
    assert_eq!(
        ids.contains(&"geometry-occt".to_string()),
        nonius_occt::available()
    );
    assert_eq!(
        nonius_occt::occt_version().is_some(),
        nonius_occt::available()
    );
}
