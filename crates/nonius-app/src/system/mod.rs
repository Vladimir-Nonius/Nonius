//! Работа с системой: какие программы открывают файл, запуск, папка файла.
//!
//! Всё, что зависит от Windows, лежит в `windows.rs`. Другая система добавляется
//! своим файлом с теми же функциями; пока вместо него запасной `other`.

use std::path::Path;

#[cfg(not(windows))]
mod other;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use other as platform;
#[cfg(windows)]
use windows as platform;

/// Программа, которая умеет открывать файл.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// Название для показа.
    pub name: String,
    /// Строка запуска из настроек системы: `"C:\…\app.exe" "%1"`.
    pub command: String,
}

/// Чем открыть файл.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenWay {
    /// Программой, назначенной в системе.
    Default,
    /// Этой программой.
    Program(Program),
    /// Спросить в стандартном окне системы «Открыть с помощью».
    Choose,
    /// Не открывать, а показать в папке.
    Folder,
}

/// Программы, которые система знает для такого файла. Диск не сканируется:
/// список берётся из настроек системы. Первой идёт назначенная программа.
pub fn programs_for(file: &str) -> Vec<Program> {
    match Path::new(file).extension().and_then(|e| e.to_str()) {
        Some(extension) => platform::programs(&extension.to_lowercase()),
        None => Vec::new(),
    }
}

/// Выполнить `way` для файла.
pub(crate) fn open(file: &Path, way: &OpenWay) -> std::io::Result<()> {
    match way {
        OpenWay::Default => opener::open(file).map_err(std::io::Error::other),
        OpenWay::Program(program) => platform::launch(program, file),
        OpenWay::Choose => platform::choose(file),
        OpenWay::Folder => platform::reveal(file),
    }
}
