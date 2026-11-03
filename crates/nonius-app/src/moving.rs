//! Перенести проект в другую папку целиком, с историей: так черновик окна
//! становится настоящим проектом. Название проекта — имя новой папки.

use std::fs;
use std::path::Path;

use nonius_core::project::Project;

use crate::error::{Error, Result};

/// Перенести проект из `from` в `to`. Папки `to` нет или она пустая.
/// Проект в этот момент не должен быть открыт.
pub fn move_project(from: &Path, to: &Path) -> Result<()> {
    Project::open(from)?;
    if to.exists() {
        let empty = fs::read_dir(to)
            .map_err(|e| Error::io(to, e))?
            .next()
            .is_none();
        if !empty {
            return Err(Error::FolderNotEmpty(to.to_path_buf()));
        }
        fs::remove_dir(to).map_err(|e| Error::io(to, e))?;
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    // На другой диск папку не переименовать: тогда копия и удаление.
    if fs::rename(from, to).is_err() {
        copy_dir(from, to)?;
        fs::remove_dir_all(from).map_err(|e| Error::io(from, e))?;
    }
    let mut project = Project::open(to)?;
    if let Some(name) = to.file_name() {
        let name = name.to_string_lossy().into_owned();
        project.update_config(|c| c.name = name)?;
    }
    Ok(())
}

/// Скопировать папку со всем содержимым.
fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to).map_err(|e| Error::io(to, e))?;
    for entry in fs::read_dir(from).map_err(|e| Error::io(from, e))? {
        let entry = entry.map_err(|e| Error::io(from, e))?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if entry.file_type().map_err(|e| Error::io(&src, e))?.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| Error::io(&src, e))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_moves_with_history_and_takes_the_folder_name() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("черновик").join("Без имени");
        crate::create_project(&from, None).unwrap();
        let to = tmp.path().join("Мастерская");
        fs::create_dir(&to).unwrap();
        move_project(&from, &to).unwrap();
        assert!(!from.exists());
        assert!(to.join(".git").is_dir());
        assert_eq!(Project::open(&to).unwrap().config().name, "Мастерская");
    }

    #[test]
    fn full_folder_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("Без имени");
        crate::create_project(&from, None).unwrap();
        let to = tmp.path().join("занято");
        fs::create_dir(&to).unwrap();
        fs::write(to.join("чужое.txt"), "x").unwrap();
        let e = move_project(&from, &to).unwrap_err();
        assert_eq!(e.code(), "already_exists");
        assert!(from.join("project.yaml").is_file(), "черновик на месте");
    }
}
