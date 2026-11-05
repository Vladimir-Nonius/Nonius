//! Обход папок проекта: где детали и где папки-группы. Деталь — папка с
//! `meta.yaml`, внутрь неё обход не заходит; остальные папки — группы.
//! Скрытые папки (`.git`, …) пропускаются. Обычные файлы в папках-группах
//! (заметки, чертежи, ТЗ) тоже видны: проект — это папки на диске. Здесь же — проект старого
//! устройства (`parts/`, `assemblies/`): он только для просмотра.

use std::fs;
use std::path::{Component, Path, PathBuf};

use super::agents::AGENTS_FILE;
use super::{
    Item, Project, ASSEMBLIES_DIR, KEEP_FILE, META_FILE, PARTS_DIR, PROJECT_FILE, TREE_SCHEMA,
};
use crate::error::{Error, Result};
use crate::meta::ItemKind;
use crate::names::PartName;
use crate::schema;

/// Глубже этого папки не смотрим: защита от зацикленных ссылок.
const MAX_DEPTH: usize = 32;

/// Что лежит в проекте.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    /// Детали и сборки в порядке обхода.
    pub items: Vec<Item>,
    /// Папки-группы: путь от корня через `/`, родитель раньше детей.
    pub folders: Vec<String>,
    /// Файлы в папках-группах и в корне, путь от корня через `/`. Служебных
    /// (`project.yaml`, `AGENTS.md`, `.gitkeep`, скрытых) здесь нет.
    pub files: Vec<String>,
}

impl Project {
    /// Проект старого устройства (`parts/`, `assemblies/`): пока его не
    /// обновили, менять в нём ничего нельзя.
    pub fn is_old_layout(&self) -> bool {
        self.old_layout
    }

    /// Ошибка [`Error::OldLayout`], если проект ещё старого устройства. Её
    /// проверяет каждое действие, которое что-то меняет.
    pub fn writable(&self) -> Result<()> {
        if self.old_layout {
            return Err(Error::OldLayout(self.root.clone()));
        }
        Ok(())
    }

    /// Записать `project.yaml` новой схемы, когда детали уже перенесены из
    /// `parts/` и `assemblies/`. Переносит их `nonius-store`.
    pub fn finish_upgrade(&mut self) -> Result<()> {
        self.config.schema = TREE_SCHEMA;
        schema::save(&self.config, &self.root.join(PROJECT_FILE))?;
        self.old_layout = false;
        Ok(())
    }

    /// Обойти папки проекта.
    pub fn scan(&self) -> Result<Layout> {
        let mut out = Layout::default();
        self.walk(&self.root, "", 0, &mut out)?;
        Ok(out)
    }

    fn walk(&self, dir: &Path, rel: &str, depth: usize, out: &mut Layout) -> Result<()> {
        if depth > MAX_DEPTH {
            return Ok(());
        }
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(Error::io(dir, e)),
        };
        let mut dirs: Vec<(String, PathBuf)> = Vec::new();
        let mut files: Vec<String> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| Error::io(dir, e))?;
            let path = entry.path();
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                dirs.push((name, path));
            } else if !is_service(rel, &name) {
                files.push(join(rel, &name));
            }
        }
        dirs.sort();
        files.sort();
        out.files.extend(files);
        for (name, path) in dirs {
            let child = join(rel, &name);
            if path.join(META_FILE).is_file() {
                if let Ok(name) = PartName::new(name) {
                    let kind = kind_of(&path);
                    out.items.push(Item {
                        name,
                        kind,
                        dir: path,
                    });
                }
                continue;
            }
            out.folders.push(child.clone());
            self.walk(&path, &child, depth + 1, out)?;
        }
        Ok(())
    }

    /// Папка-группа `folder` (путь от корня, пустой — корень) на диске.
    /// Путь не выходит из проекта и не ведёт внутрь детали.
    pub fn folder_dir(&self, folder: &Path) -> Result<PathBuf> {
        let mut dir = self.root.clone();
        for part in folder.components() {
            match part {
                Component::Normal(p) if !p.to_string_lossy().starts_with('.') => {
                    dir.push(p);
                    if dir.join(META_FILE).is_file() {
                        return Err(Error::InsideItem(dir));
                    }
                }
                Component::CurDir => {}
                _ => {
                    return Err(Error::InvalidName {
                        name: folder.display().to_string(),
                        reason: "папка должна быть внутри проекта",
                    })
                }
            }
        }
        Ok(dir)
    }

    /// Создать папку-группу `folder` (путь от корня). В пустую папку кладётся
    /// [`KEEP_FILE`], иначе git её не сохранит.
    pub fn create_folder(&self, folder: &Path) -> Result<PathBuf> {
        self.writable()?;
        let dir = self.folder_dir(folder)?;
        if dir == self.root {
            return Err(Error::InvalidName {
                name: String::new(),
                reason: "у папки должно быть имя",
            });
        }
        if dir.exists() {
            return Err(Error::AlreadyExists(dir));
        }
        fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let keep = dir.join(KEEP_FILE);
        fs::write(&keep, "").map_err(|e| Error::io(&keep, e))?;
        Ok(dir)
    }
}

/// Папка детали `name` в проекте старого устройства, путь от корня через `/`.
pub fn old_item_path(kind: ItemKind, name: &PartName) -> String {
    let base = match kind {
        ItemKind::Part => PARTS_DIR,
        ItemKind::Assembly => ASSEMBLIES_DIR,
    };
    format!("{base}/{name}")
}

/// Путь от корня через `/`: `rel` и имя внутри него.
fn join(rel: &str, name: &str) -> String {
    if rel.is_empty() {
        name.to_string()
    } else {
        format!("{rel}/{name}")
    }
}

/// Служебный файл Nonius: в списке файлов проекта его не показывают.
fn is_service(rel: &str, name: &str) -> bool {
    name == KEEP_FILE || (rel.is_empty() && (name == PROJECT_FILE || name == AGENTS_FILE))
}

/// Вид детали из её `meta.yaml`; не прочитался — деталь.
fn kind_of(dir: &Path) -> ItemKind {
    let kind = fs::read_to_string(dir.join(META_FILE))
        .ok()
        .and_then(|text| serde_yaml::from_str::<serde_yaml::Value>(&text).ok())
        .and_then(|doc| doc.get("kind").cloned())
        .and_then(|kind| serde_yaml::from_value(kind).ok());
    kind.unwrap_or_default()
}
