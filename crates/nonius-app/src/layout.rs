//! Папки проекта: карта «что где лежит», папки-группы, перенос детали и
//! перевод старого проекта (`parts/`, `assemblies/`) на папки.

use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::meta::ItemKind;
use nonius_core::project::Item;
use nonius_core::PartName;

use crate::system::{self, OpenWay};
use crate::{App, Error, Result};

/// Карта проекта: папки и где лежит каждая деталь. Это просто обход диска,
/// отдельно она нигде не хранится.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectMap {
    /// Папки-группы, путь от корня через `/`, родитель раньше детей.
    pub folders: Vec<String>,
    /// Детали и сборки по порядку папок.
    pub items: Vec<MapItem>,
    /// Обычные файлы в папках-группах и в корне, путь от корня через `/`.
    pub files: Vec<String>,
}

/// Деталь на карте проекта.
#[derive(Debug, Clone, PartialEq)]
pub struct MapItem {
    /// Имя.
    pub name: PartName,
    /// Деталь или сборка.
    pub kind: ItemKind,
    /// Папка детали, путь от корня через `/`.
    pub path: String,
}

impl App {
    /// Проект старого устройства: детали в `parts/` и `assemblies/`, менять
    /// ничего нельзя, пока его не обновят ([`App::upgrade`]).
    pub fn is_old_layout(&self) -> bool {
        self.project.is_old_layout()
    }

    /// Перевести старый проект на папки: детали и сборки переезжают в корень
    /// одной записью истории. После этого проект надо открыть заново.
    pub fn upgrade(&self) -> Result<()> {
        self.store()?.upgrade()?;
        Ok(())
    }

    /// Карта проекта.
    pub fn map(&self) -> Result<ProjectMap> {
        let layout = self.project.scan()?;
        let root = self.project.root();
        let items = layout
            .items
            .into_iter()
            .map(|item| MapItem {
                path: rel_path(root, &item.dir),
                name: item.name,
                kind: item.kind,
            })
            .collect();
        Ok(ProjectMap {
            folders: layout.folders,
            items,
            files: layout.files,
        })
    }

    /// Создать папку-группу (путь от корня, можно сразу несколько уровней).
    pub fn new_folder(&self, folder: &Path) -> Result<PathBuf> {
        Ok(self.project.create_folder(folder)?)
    }

    /// Добавить деталь или сборку в папку (путь от корня, пустой — корень).
    pub fn new_item_in(&self, name: &str, kind: ItemKind, folder: &Path) -> Result<Item> {
        Ok(self
            .project
            .create_item_in(kind, &PartName::new(name)?, folder)?)
    }

    /// Положить копию своего файла в папку-группу (путь от корня, пустой —
    /// корень) и сразу сохранить её в истории проекта. Возвращает путь файла
    /// от корня через `/`. Файл с таким именем там уже есть — ошибка.
    pub fn add_folder_file(&self, folder: &Path, file: &Path) -> Result<String> {
        self.project.writable()?;
        let name = match file.file_name() {
            Some(n) if file.is_file() => n.to_string_lossy().into_owned(),
            _ => return Err(Error::FileNotFound(file.to_path_buf())),
        };
        let dir = self.project.folder_dir(folder)?;
        let root = self.project.root();
        let dest = dir.join(&name);
        let path = rel_path(root, &dest);
        if dest.exists() {
            let place = rel_path(root, &dir);
            let item = if place.is_empty() {
                "корень проекта".into()
            } else {
                place
            };
            return Err(Error::FileExists { item, path: name });
        }
        let store = self.store()?;
        fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        fs::copy(file, &dest).map_err(|e| Error::io(&dest, e))?;
        let message = format!(
            "{path}: файл добавлен

Добавлено в Nonius.
"
        );
        if let Err(e) = store.commit_project_file(&path, &message) {
            // Не сохранилось в истории — копию не оставляем: её никто не ждёт.
            let _ = fs::remove_file(&dest);
            return Err(e.into());
        }
        Ok(path)
    }

    /// Открыть файл из папки-группы (путь от корня через `/`) программой или
    /// показать его в папке. Возвращает, что именно открыто.
    pub fn open_folder_file(&self, path: &str, way: &OpenWay) -> Result<PathBuf> {
        let file = self.project.folder_dir(Path::new(path))?;
        if !file.is_file() {
            return Err(Error::FileNotFound(file));
        }
        system::open(&file, way).map_err(|e| Error::OpenFailed {
            path: file.clone(),
            why: e.to_string(),
        })?;
        Ok(file)
    }

    /// Перенести деталь в папку (путь от корня, пустой — корень). Версии
    /// остаются при ней; несохранённое надо сначала сохранить.
    pub fn move_item(&self, name: &str, folder: &Path) -> Result<Item> {
        let item = self.item(name)?;
        Ok(self.store()?.move_item(&item, folder)?)
    }
}

/// Путь от корня проекта через `/`.
fn rel_path(root: &Path, dir: &Path) -> String {
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use nonius_core::extension::Registry;

    #[test]
    fn file_goes_into_folder_and_history() {
        for (k, v) in [
            ("GIT_AUTHOR_NAME", "Тестировщик"),
            ("GIT_AUTHOR_EMAIL", "test@example.com"),
            ("GIT_COMMITTER_NAME", "Тестировщик"),
            ("GIT_COMMITTER_EMAIL", "test@example.com"),
        ] {
            std::env::set_var(k, v);
        }
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Мастерская");
        crate::create_project(&root, Some("Мастерская")).unwrap();
        let app = App::open(&root, Registry::new()).unwrap();
        app.new_folder(Path::new("Стена")).unwrap();
        let mine = tmp.path().join("ТЗ стены.pdf");
        fs::write(&mine, "чертёж").unwrap();

        let path = app.add_folder_file(Path::new("Стена"), &mine).unwrap();
        assert_eq!(path, "Стена/ТЗ стены.pdf");
        assert!(mine.is_file(), "свой файл остаётся на месте");
        let map = app.map().unwrap();
        assert!(map.files.contains(&path), "{:?}", map.files);
        assert!(!map
            .files
            .iter()
            .any(|f| f.ends_with(".gitkeep") || f == "project.yaml"));
        let e = app.add_folder_file(Path::new("Стена"), &mine).unwrap_err();
        assert_eq!(e.code(), "already_exists");
        let status = std::process::Command::new("git")
            .args([
                "-C",
                &root.to_string_lossy(),
                "status",
                "--porcelain",
                "--",
                "Стена/ТЗ стены.pdf",
            ])
            .output()
            .unwrap();
        assert!(status.stdout.is_empty(), "файл сохранён в истории");
    }
}
