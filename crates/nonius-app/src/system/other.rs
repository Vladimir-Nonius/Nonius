//! Запасной вариант для систем, которые Nonius пока не знает: программу выбирает
//! сама система, папка открывается без выделения файла.

use std::io::{Error, ErrorKind, Result};
use std::path::Path;

use super::Program;

pub(super) fn programs(_extension: &str) -> Vec<Program> {
    Vec::new()
}

pub(super) fn launch(_program: &Program, _file: &Path) -> Result<()> {
    Err(Error::from(ErrorKind::Unsupported))
}

pub(super) fn choose(file: &Path) -> Result<()> {
    opener::open(file).map_err(Error::other)
}

pub(super) fn reveal(file: &Path) -> Result<()> {
    let folder = file.parent().unwrap_or(file);
    opener::open(folder).map_err(Error::other)
}
