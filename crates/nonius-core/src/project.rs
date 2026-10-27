//! Папка проекта и `project.yaml`.
//!
//! ```text
//! мой-проект/
//! ├── project.yaml      # имя, участники, допуски сравнения
//! ├── AGENTS.md         # описание проекта и правила для ИИ
//! ├── .gitattributes    # бинарные файлы идут через Git LFS
//! ├── parts/<имя>/      # meta.yaml, model.step, model.sldprt, tree.yaml, preview.png, docs/
//! ├── assemblies/<имя>/ # то же для сборок
//! └── notes/            # заметки Markdown
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::meta::{ItemKind, PartMeta};
use crate::names::PartName;
use crate::schema::{self, Versioned};

/// Имя файла настроек проекта.
pub const PROJECT_FILE: &str = "project.yaml";
/// Имя паспорта версии в папке детали.
pub const META_FILE: &str = "meta.yaml";
/// Имя слепка дерева построения в папке детали.
pub const TREE_FILE: &str = "tree.yaml";
/// Папка деталей.
pub const PARTS_DIR: &str = "parts";
/// Папка сборок.
pub const ASSEMBLIES_DIR: &str = "assemblies";
/// Папка заметок.
pub const NOTES_DIR: &str = "notes";
/// Папка документов детали (чертежи, PDF, DXF).
pub const DOCS_DIR: &str = "docs";

/// Расширения, которые хранятся через Git LFS.
pub const LFS_EXTENSIONS: &[&str] = &[
    "step", "stp", "sldprt", "sldasm", "slddrw", "x_t", "x_b", "stl", "3mf", "dxf", "dwg", "pdf",
    "png", "jpg", "gif",
];

/// Роль участника проекта.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Лидер: утверждает версии.
    Leader,
    /// Редактор: фиксирует версии и предлагает их.
    Editor,
    /// Читатель: смотрит и комментирует.
    Reader,
    /// ИИ-агент: по умолчанию только предлагает.
    Agent,
}

/// Участник проекта.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Member {
    /// Имя или логин.
    pub name: String,
    /// Роль.
    pub role: Role,
}

/// Настройки сравнения версий.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompareSettings {
    /// Допуск, мм: отличия меньше допуска не считаются изменением.
    #[serde(default = "CompareSettings::default_tolerance")]
    pub tolerance_mm: f64,
}

impl CompareSettings {
    fn default_tolerance() -> f64 {
        0.01
    }
}

impl Default for CompareSettings {
    fn default() -> Self {
        Self {
            tolerance_mm: Self::default_tolerance(),
        }
    }
}

/// Содержимое `project.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Версия схемы файла.
    pub schema: u32,
    /// Название проекта.
    pub name: String,
    /// Участники и их роли.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<Member>,
    /// Настройки сравнения.
    #[serde(default)]
    pub compare: CompareSettings,
}

impl Versioned for ProjectConfig {
    const KIND: &'static str = "project";
    const CURRENT: u32 = 1;
}

/// Деталь или сборка в проекте.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Имя (оно же имя папки).
    pub name: PartName,
    /// Деталь или сборка.
    pub kind: ItemKind,
    /// Папка.
    pub dir: PathBuf,
}

/// Открытый проект Nonius.
#[derive(Debug, Clone)]
pub struct Project {
    root: PathBuf,
    config: ProjectConfig,
}

impl Project {
    /// Создать проект в папке `root` (папка создаётся, если её нет).
    pub fn init(root: impl AsRef<Path>, name: impl Into<String>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let config_path = root.join(PROJECT_FILE);
        if config_path.exists() {
            return Err(Error::AlreadyExists(config_path));
        }
        for dir in [PARTS_DIR, ASSEMBLIES_DIR, NOTES_DIR] {
            let d = root.join(dir);
            fs::create_dir_all(&d).map_err(|e| Error::io(&d, e))?;
        }
        let name = name.into();
        let config = ProjectConfig {
            schema: ProjectConfig::CURRENT,
            name: name.clone(),
            members: Vec::new(),
            compare: CompareSettings::default(),
        };
        schema::save(&config, &config_path)?;
        write_if_missing(&root.join(".gitattributes"), &gitattributes())?;
        write_if_missing(&root.join("AGENTS.md"), &agents_md(&name))?;
        Ok(Self { root, config })
    }

    /// Открыть проект, корень которого — `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let config_path = root.join(PROJECT_FILE);
        if !config_path.is_file() {
            return Err(Error::NotAProject(root));
        }
        let config = schema::load(&config_path)?;
        Ok(Self { root, config })
    }

    /// Найти проект, поднимаясь от `start` к корню диска.
    pub fn discover(start: impl AsRef<Path>) -> Result<Self> {
        let start = start.as_ref();
        let mut dir = Some(start);
        while let Some(d) = dir {
            if d.join(PROJECT_FILE).is_file() {
                return Self::open(d);
            }
            dir = d.parent();
        }
        Err(Error::NotAProject(start.to_path_buf()))
    }

    /// Корень проекта.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Настройки проекта.
    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    /// Изменить настройки и сохранить `project.yaml`.
    pub fn update_config(&mut self, f: impl FnOnce(&mut ProjectConfig)) -> Result<()> {
        let mut next = self.config.clone();
        f(&mut next);
        schema::save(&next, &self.root.join(PROJECT_FILE))?;
        self.config = next;
        Ok(())
    }

    /// Папка детали или сборки (может ещё не существовать).
    pub fn item_dir(&self, kind: ItemKind, name: &PartName) -> PathBuf {
        self.root.join(kind_dir(kind)).join(name.as_str())
    }

    /// Создать деталь или сборку: папку, `docs/` и пустой `meta.yaml`.
    pub fn create_item(&self, kind: ItemKind, name: &PartName) -> Result<Item> {
        if let Some(existing) = self.find_item(name)? {
            return Err(Error::AlreadyExists(existing.dir));
        }
        let dir = self.item_dir(kind, name);
        let docs = dir.join(DOCS_DIR);
        fs::create_dir_all(&docs).map_err(|e| Error::io(&docs, e))?;
        schema::save(&PartMeta::new(kind), &dir.join(META_FILE))?;
        Ok(Item {
            name: name.clone(),
            kind,
            dir,
        })
    }

    /// Найти деталь или сборку по имени. Имена уникальны в проекте.
    pub fn find_item(&self, name: &PartName) -> Result<Option<Item>> {
        Ok(self.items()?.into_iter().find(|i| &i.name == name))
    }

    /// Все детали и сборки, отсортированные по виду и имени.
    pub fn items(&self) -> Result<Vec<Item>> {
        let mut out = Vec::new();
        for kind in [ItemKind::Part, ItemKind::Assembly] {
            let base = self.root.join(kind_dir(kind));
            let entries = match fs::read_dir(&base) {
                Ok(e) => e,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(Error::io(&base, e)),
            };
            for entry in entries {
                let entry = entry.map_err(|e| Error::io(&base, e))?;
                let dir = entry.path();
                if !dir.join(META_FILE).is_file() {
                    continue;
                }
                let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if let Ok(name) = PartName::new(name) {
                    out.push(Item { name, kind, dir });
                }
            }
        }
        out.sort_by(|a, b| (a.kind as u8, &a.name).cmp(&(b.kind as u8, &b.name)));
        Ok(out)
    }

    /// Прочитать `meta.yaml` детали.
    pub fn load_meta(&self, item: &Item) -> Result<PartMeta> {
        schema::load(&item.dir.join(META_FILE))
    }
}

fn kind_dir(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Part => PARTS_DIR,
        ItemKind::Assembly => ASSEMBLIES_DIR,
    }
}

fn write_if_missing(path: &Path, text: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, text).map_err(|e| Error::io(path, e))
}

/// Шаблон без учёта регистра: `step` → `[sS][tT][eE][pP]`.
fn case_insensitive(ext: &str) -> String {
    ext.chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                format!("[{}{}]", c.to_ascii_lowercase(), c.to_ascii_uppercase())
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn gitattributes() -> String {
    let mut s = String::from(
        "# Создано Nonius.\n\
         # Текстовые файлы проекта — с переводом строки LF на любой ОС.\n\
         *.yaml text eol=lf\n\
         *.md text eol=lf\n\
         # Бинарные файлы CAD хранятся через Git LFS.\n",
    );
    for ext in LFS_EXTENSIONS {
        s.push_str(&format!(
            "*.{} filter=lfs diff=lfs merge=lfs -text\n",
            case_insensitive(ext)
        ));
    }
    s
}

fn agents_md(name: &str) -> String {
    format!(
        "# {name}\n\n\
Проект ведётся в Nonius — системе контроля версий для CAD.\n\n\
## Как устроен проект\n\
- `parts/<имя>/` и `assemblies/<имя>/` — детали и сборки.\n\
  - `meta.yaml` — масса, объём, габариты, материал, свойства.\n\
  - `tree.yaml` — дерево построения (если модель из SolidWorks).\n\
  - `model.step` — геометрия, `docs/` — чертежи, PDF, DXF.\n\
- `notes/` — заметки. Ссылки: `[[деталь]]`, `[[деталь@v3]]`, `[[деталь#Фича]]`.\n\n\
## Правила для ИИ\n\
- Читай `meta.yaml`, `tree.yaml` и заметки — модели открывать не нужно.\n\
- Изменения предлагай как вариант; в основную линию — только если лидер разрешил.\n\
- Не удаляй историю и не снимай чужие блокировки.\n\
- Пиши, почему сделано изменение, а не только что изменено.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitattributes_cover_both_cases() {
        let g = gitattributes();
        assert!(g.contains("*.[sS][tT][eE][pP] filter=lfs"));
        assert!(g.contains("*.[xX]_[tT] filter=lfs"));
    }
}
