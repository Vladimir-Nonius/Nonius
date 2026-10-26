//! Поиск OpenCascade и сборка C++-прослойки.
//!
//! OpenCascade ищется в `NONIUS_OCCT_INCLUDE` / `NONIUS_OCCT_LIB`, затем в
//! обычных системных папках. Если его нет, крейт собирается без геометрии
//! (`nonius_occt::available()` вернёт `false`), а программа работает дальше.

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo::rustc-check-cfg=cfg(nonius_occt)");
    println!("cargo::rerun-if-changed=cpp/nonius_occt.cpp");
    println!("cargo::rerun-if-changed=cpp/nonius_occt.h");
    for var in [
        "NONIUS_OCCT_INCLUDE",
        "NONIUS_OCCT_LIB",
        "NONIUS_OCCT_DISABLE",
    ] {
        println!("cargo::rerun-if-env-changed={var}");
    }
    if env::var_os("NONIUS_OCCT_DISABLE").is_some() {
        return;
    }
    let Some((include, lib)) = find() else {
        println!(
            "cargo::warning=OpenCascade не найден — геометрия отключена. \
             Укажите NONIUS_OCCT_INCLUDE и NONIUS_OCCT_LIB."
        );
        return;
    };

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .file("cpp/nonius_occt.cpp")
        .include(&include)
        .warnings(false);
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // Исходник в UTF-8 (русские сообщения), исключения C++ включены.
        build.flag("/utf-8").flag("/EHsc");
    }
    build.compile("nonius_occt");

    println!("cargo::rustc-link-search=native={}", lib.display());
    for name in libraries(&lib) {
        println!("cargo::rustc-link-lib=dylib={name}");
    }
    println!("cargo::rustc-cfg=nonius_occt");
}

fn find() -> Option<(PathBuf, PathBuf)> {
    let env_include = env::var_os("NONIUS_OCCT_INCLUDE").map(PathBuf::from);
    let env_lib = env::var_os("NONIUS_OCCT_LIB").map(PathBuf::from);

    let mut includes: Vec<PathBuf> = env_include.into_iter().collect();
    includes.extend(
        [
            "/usr/include/opencascade",
            "/usr/local/include/opencascade",
            "/opt/homebrew/include/opencascade",
        ]
        .map(PathBuf::from),
    );
    let include = includes
        .into_iter()
        .find(|d| d.join("Standard_Version.hxx").is_file())?;

    let mut libs: Vec<PathBuf> = env_lib.into_iter().collect();
    if let Some(prefix) = include.parent().and_then(Path::parent) {
        libs.push(prefix.join("lib"));
        libs.push(prefix.join("lib64"));
    }
    libs.extend(
        [
            "/usr/lib/x86_64-linux-gnu",
            "/usr/lib/aarch64-linux-gnu",
            "/usr/lib64",
            "/usr/lib",
            "/usr/local/lib",
            "/opt/homebrew/lib",
        ]
        .map(PathBuf::from),
    );
    let lib = libs.into_iter().find(|d| has_lib(d, "TKernel"))?;
    Some((include, lib))
}

fn has_lib(dir: &Path, name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    let candidates = [
        format!("lib{name}.so"),
        format!("lib{name}.dylib"),
        format!("lib{name}.a"),
        format!("{name}.lib"),
    ];
    entries.flatten().any(|e| {
        let f = e.file_name().to_string_lossy().into_owned();
        candidates
            .iter()
            .any(|c| f == *c || f.starts_with(&format!("{c}.")))
    })
}

/// Нужные библиотеки. В OpenCascade 7.8 STEP собран в TKDESTEP,
/// в более старых — в TKSTEP и соседях.
fn libraries(dir: &Path) -> Vec<&'static str> {
    let mut names = vec![
        "TKernel",
        "TKMath",
        "TKG2d",
        "TKG3d",
        "TKGeomBase",
        "TKBRep",
        "TKGeomAlgo",
        "TKTopAlgo",
        "TKPrim",
        "TKShHealing",
        "TKBO",
        "TKBool",
        "TKMesh",
        "TKXSBase",
    ];
    if has_lib(dir, "TKDESTEP") {
        names.extend(["TKDESTEP", "TKDE"]);
    } else {
        names.extend(["TKSTEP", "TKSTEPBase", "TKSTEPAttr", "TKSTEP209"]);
    }
    names.retain(|n| has_lib(dir, n));
    names
}
