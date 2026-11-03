//! Папка проекта и `project.yaml`.
//!
//! ```text
//! мой-проект/
//! ├── project.yaml      # имя, участники, допуски сравнения
//! ├── AGENTS.md         # описание проекта и правила для ИИ
//! ├── .gitattributes    # бинарные файлы идут через Git LFS
//! ├── Стена/            # папка-группа: обычная папка, сколько угодно уровней
//! │   └── PANEL-00/     # деталь или сборка: папка с meta.yaml, model.step, docs/ …
//! ├── RAIL-01/          # деталь может лежать и прямо в корне
//! └── notes/            # заметки Markdown
//! ```
//!
//! Дерево проекта и есть папки на диске: отдельного списка «что где лежит»
//! нет. До схемы 2 детали лежали в `parts/<имя>/`, а сборки — в
//! `assemblies/<имя>/`; такой проект открывается только для просмотра, пока
//! его не обновят (`nonius-store`: перенос в корень одной записью истории).
//!
//! Обход папок — в `layout.rs`, шаблоны `.gitattributes` и `AGENTS.md`
//! нового проекта — в `templates.rs`.

mod agents;
mod layout;
mod templates;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::extension::Snapshot;
use crate::meta::{ItemKind, PartMeta};
use crate::names::PartName;
use crate::schema::{self, Versioned};
use agents::agents_md;
pub use agents::AGENTS_FILE;
pub use layout::{old_item_path, Layout};
use templates::gitattributes;

/// Имя файла настроек проекта.
pub const PROJECT_FILE: &str = "project.yaml";
/// Имя паспорта версии в папке детали.
pub const META_FILE: &str = "meta.yaml";
/// Имя слепка дерева построения в папке детали.
pub const TREE_FILE: &str = "tree.yaml";
/// Папка деталей в проекте старого устройства (схема 1).
pub const PARTS_DIR: &str = "parts";
/// Папка сборок в проекте старого устройства (схема 1).
pub const ASSEMBLIES_DIR: &str = "assemblies";
/// Пустой файл, который держит в git пустую папку-группу.
pub const KEEP_FILE: &str = ".gitkeep";
/// Схема `project.yaml`, с которой детали лежат в папках проекта.
pub const TREE_SCHEMA: u32 = 2;
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
    const CURRENT: u32 = TREE_SCHEMA;

    /// Поля схем 1 и 2 одинаковые: схема 2 говорит только, что детали лежат
    /// в папках проекта. Переносит их `nonius-store` вместе с записью истории.
    fn migrate(
        doc: serde_yaml::Value,
        from: u32,
    ) -> std::result::Result<serde_yaml::Value, String> {
        match from {
            1 => Ok(doc),
            _ => Err(format!("нет миграции со схемы {from}")),
        }
    }
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
    /// Детали в `parts/` и `assemblies/`: проект ещё не обновлён.
    old_layout: bool,
}

impl Project {
    /// Создать проект в папке `root` (папка создаётся, если её нет).
    pub fn init(root: impl AsRef<Path>, name: impl Into<String>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let config_path = root.join(PROJECT_FILE);
        if config_path.exists() {
            return Err(Error::AlreadyExists(config_path));
        }
        let notes = root.join(NOTES_DIR);
        fs::create_dir_all(&notes).map_err(|e| Error::io(&notes, e))?;
        let name = name.into();
        let config = ProjectConfig {
            schema: ProjectConfig::CURRENT,
            name: name.clone(),
            members: Vec::new(),
            compare: CompareSettings::default(),
        };
        schema::save(&config, &config_path)?;
        write_if_missing(&root.join(".gitattributes"), &gitattributes())?;
        write_if_missing(&root.join(AGENTS_FILE), &agents_md(&name))?;
        Ok(Self {
            root,
            config,
            old_layout: false,
        })
    }

    /// Открыть проект, корень которого — `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let config_path = root.join(PROJECT_FILE);
        if !config_path.is_file() {
            return Err(Error::NotAProject(root));
        }
        let text = fs::read_to_string(&config_path).map_err(|e| Error::io(&config_path, e))?;
        let config = schema::from_str(&text, &config_path)?;
        Ok(Self {
            root,
            config,
            old_layout: schema::version_in(&text) < TREE_SCHEMA,
        })
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
        self.writable()?;
        let mut next = self.config.clone();
        f(&mut next);
        schema::save(&next, &self.root.join(PROJECT_FILE))?;
        self.config = next;
        Ok(())
    }

    /// Создать деталь или сборку в корне проекта.
    pub fn create_item(&self, kind: ItemKind, name: &PartName) -> Result<Item> {
        self.create_item_in(kind, name, Path::new(""))
    }

    /// Создать деталь или сборку в папке `folder` (путь от корня проекта,
    /// пустой — корень): папку детали, `docs/` и пустой `meta.yaml`.
    pub fn create_item_in(&self, kind: ItemKind, name: &PartName, folder: &Path) -> Result<Item> {
        self.writable()?;
        if let Some(existing) = self.find_item(name)? {
            return Err(Error::AlreadyExists(existing.dir));
        }
        let dir = self.folder_dir(folder)?.join(name.as_str());
        if dir.exists() {
            return Err(Error::AlreadyExists(dir));
        }
        let docs = dir.join(DOCS_DIR);
        fs::create_dir_all(&docs).map_err(|e| Error::io(&docs, e))?;
        schema::save(&PartMeta::new(kind), &dir.join(META_FILE))?;
        Ok(Item {
            name: name.clone(),
            kind,
            dir,
        })
    }

    /// Найти деталь или сборку по имени. Имена уникальны в проекте с точностью
    /// до регистра: на Windows `Вал` и `вал` — одна папка.
    pub fn find_item(&self, name: &PartName) -> Result<Option<Item>> {
        Ok(self.items()?.into_iter().find(|i| i.name.same_as(name)))
    }

    /// Все детали и сборки, отсортированные по виду и имени.
    pub fn items(&self) -> Result<Vec<Item>> {
        let mut out = self.scan()?.items;
        out.sort_by(|a, b| (a.kind as u8, &a.name).cmp(&(b.kind as u8, &b.name)));
        Ok(out)
    }

    /// Прочитать `meta.yaml` детали.
    pub fn load_meta(&self, item: &Item) -> Result<PartMeta> {
        schema::load(&item.dir.join(META_FILE))
    }

    /// Текущее (ещё не зафиксированное) состояние детали, как его видят модули.
    pub fn working_snapshot(&self, item: &Item) -> Result<Snapshot> {
        let tree_path = item.dir.join(TREE_FILE);
        let tree = if tree_path.is_file() {
            Some(schema::load(&tree_path)?)
        } else {
            None
        };
        Ok(Snapshot {
            item: item.name.clone(),
            version: None,
            dir: item.dir.clone(),
            meta: self.load_meta(item)?,
            tree,
        })
    }
}

fn write_if_missing(path: &Path, text: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, text).map_err(|e| Error::io(path, e))
}
