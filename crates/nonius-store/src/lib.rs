//! Хранилище версий Nonius на git + Git LFS.
//!
//! Каждая версия детали — коммит git, который меняет только папку этой детали.
//! В сообщении коммита — описание версии и служебные строки:
//!
//! ```text
//! кронштейн v3: толщина 4 → 5 мм под новый вал
//!
//! Подробности для истории.
//!
//! Nonius-Item: кронштейн
//! Nonius-Version: v3
//! ```
//!
//! Поэтому историю можно смотреть и обычным `git log`, а бинарные файлы
//! (STEP, SLDPRT, PDF) хранятся через Git LFS по `.gitattributes` проекта.
//! Ничего не удаляется: откат создаёт новую версию.

mod error;
mod git;

use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::extension::Snapshot;
use nonius_core::meta::PartMeta;
use nonius_core::project::{Item, DOCS_DIR, META_FILE, TREE_FILE};
use nonius_core::schema;
use nonius_core::tree::FeatureTree;
use nonius_core::{PartName, Project, VersionNumber};

pub use error::{Result, StoreError};
use git::{git_path, literal, Git};

const ITEM_TRAILER: &str = "Nonius-Item: ";
const VERSION_TRAILER: &str = "Nonius-Version: ";

/// Настройки доступа к git.
#[derive(Debug, Clone, Default)]
pub struct GitOptions {
    /// Имя и почта автора версий. Если не заданы — берутся из настроек git.
    pub identity: Option<(String, String)>,
}

/// Зафиксированная версия детали.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    /// Деталь.
    pub item: PartName,
    /// Номер версии.
    pub version: VersionNumber,
    /// Коммит git.
    pub commit: String,
    /// Автор.
    pub author: String,
    /// Дата в формате ISO 8601.
    pub date: String,
    /// Описание версии (без служебных строк).
    pub message: String,
}

impl VersionInfo {
    /// Первая строка описания.
    pub fn summary(&self) -> &str {
        self.message.lines().next().unwrap_or_default()
    }

    /// Короткий хеш коммита.
    pub fn short_commit(&self) -> &str {
        &self.commit[..self.commit.len().min(8)]
    }
}

/// Что изменилось в папке детали с последней версии.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeKind {
    /// Новый файл.
    Added,
    /// Изменённый файл.
    Modified,
    /// Удалённый файл.
    Deleted,
}

/// Изменённый файл детали.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Вид изменения.
    pub kind: FileChangeKind,
    /// Путь внутри папки детали: `model.step`, `docs/чертёж.pdf`.
    pub path: String,
}

/// Хранилище версий проекта.
pub struct Store {
    project: Project,
    git: Git,
    lfs: bool,
}

impl Store {
    /// Открыть хранилище проекта. Если git-репозитория ещё нет — создаёт его
    /// и фиксирует настройки проекта первым коммитом.
    pub fn open(project: Project, options: &GitOptions) -> Result<Self> {
        let root = project.root().to_path_buf();
        let git = Git::new(&root, options.identity.as_ref());

        if !root.join(".git").exists() {
            git.run(&["init", "-q", "-b", "main"])?;
        }
        let lfs = git.ok(&["lfs", "version"]);
        if lfs {
            git.run(&["lfs", "install", "--local"])?;
        }

        let store = Self { project, git, lfs };
        if !store.git.ok(&["rev-parse", "-q", "--verify", "HEAD"]) {
            store.initial_commit()?;
        }
        Ok(store)
    }

    fn initial_commit(&self) -> Result<()> {
        let files: Vec<&str> = ["project.yaml", ".gitattributes", "AGENTS.md"]
            .into_iter()
            .filter(|f| self.project.root().join(f).exists())
            .collect();
        let mut add = vec!["add", "--"];
        add.extend(&files);
        self.git.run(&add)?;
        let message = format!(
            "Проект «{}» создан\n\nСоздано Nonius.\n",
            self.project.config().name
        );
        self.git.run_with_input(
            &["commit", "-q", "--allow-empty", "-F", "-"],
            Some(message.as_bytes()),
        )?;
        Ok(())
    }

    /// Проект.
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Включён ли Git LFS. Без него бинарные файлы хранятся прямо в git —
    /// работает, но репозиторий быстрее растёт.
    pub fn lfs_enabled(&self) -> bool {
        self.lfs
    }

    /// Найти деталь или сборку по имени.
    pub fn item(&self, name: &str) -> Result<Item> {
        let parsed = PartName::new(name)?;
        self.project
            .find_item(&parsed)?
            .ok_or_else(|| StoreError::UnknownItem(name.to_string()))
    }

    fn pathspec(&self, item: &Item) -> String {
        git_path(self.project.root(), &item.dir)
    }

    /// Все версии детали, новые первыми.
    pub fn versions(&self, item: &Item) -> Result<Vec<VersionInfo>> {
        let path = self.pathspec(item);
        let out = self.git.run(&[
            "log",
            "-z",
            "--format=%H%x1f%an%x1f%aI%x1f%B",
            "--",
            &literal(&path),
        ])?;
        let text = String::from_utf8_lossy(&out);
        Ok(text
            .split('\0')
            .filter_map(|record| parse_record(record, &item.name))
            .collect())
    }

    /// Последняя версия детали.
    pub fn latest(&self, item: &Item) -> Result<Option<VersionInfo>> {
        Ok(self.versions(item)?.into_iter().next())
    }

    /// Найти версию по номеру.
    pub fn version(&self, item: &Item, version: VersionNumber) -> Result<VersionInfo> {
        self.versions(item)?
            .into_iter()
            .find(|v| v.version == version)
            .ok_or_else(|| StoreError::UnknownVersion {
                item: item.name.clone(),
                version,
            })
    }

    /// Изменения в папке детали с последней версии.
    pub fn changes(&self, item: &Item) -> Result<Vec<FileChange>> {
        let path = self.pathspec(item);
        let out = self.git.run(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            &literal(&path),
        ])?;
        let text = String::from_utf8_lossy(&out);
        let prefix = format!("{path}/");
        let mut entries = text.split('\0').filter(|e| !e.is_empty());
        let mut changes = Vec::new();
        while let Some(entry) = entries.next() {
            if entry.len() < 4 {
                continue;
            }
            let (code, file) = entry.split_at(3);
            let code = code.as_bytes();
            if matches!(code[0], b'R' | b'C') {
                entries.next(); // старое имя при переименовании
            }
            let kind = if code[0] == b'?' || code[0] == b'A' {
                FileChangeKind::Added
            } else if code[0] == b'D' || code[1] == b'D' {
                FileChangeKind::Deleted
            } else {
                FileChangeKind::Modified
            };
            let rel = file.strip_prefix(&prefix).unwrap_or(file).to_string();
            changes.push(FileChange { kind, path: rel });
        }
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(changes)
    }

    /// Зафиксировать текущее состояние папки детали как новую версию.
    pub fn commit(&self, item: &Item, message: &str) -> Result<VersionInfo> {
        let message = message.trim();
        if message.is_empty() {
            return Err(StoreError::EmptyMessage);
        }
        if self.changes(item)?.is_empty() {
            return Err(StoreError::NoChanges(item.name.clone()));
        }
        let next = match self.latest(item)? {
            Some(v) => v.version.next(),
            None => VersionNumber::FIRST,
        };
        let path = literal(&self.pathspec(item));
        self.git.run(&["add", "-A", "--", &path])?;
        let text = commit_message(&item.name, next, message);
        self.git.run_with_input(
            &["commit", "-q", "-F", "-", "--", &path],
            Some(text.as_bytes()),
        )?;
        self.version(item, next)
    }

    /// Вернуть деталь к версии `version`. Создаёт новую версию с содержимым
    /// старой; история не меняется.
    pub fn restore(
        &self,
        item: &Item,
        version: VersionNumber,
        note: Option<&str>,
    ) -> Result<VersionInfo> {
        if !self.changes(item)?.is_empty() {
            return Err(StoreError::Dirty(item.name.clone()));
        }
        let target = self.version(item, version)?;
        let path = literal(&self.pathspec(item));
        self.git
            .run(&["rm", "-r", "-q", "--ignore-unmatch", "--", &path])?;
        self.git.run(&["checkout", &target.commit, "--", &path])?;
        let docs = item.dir.join(DOCS_DIR);
        fs::create_dir_all(&docs).map_err(|e| StoreError::Io {
            path: docs,
            source: e,
        })?;

        if self.changes(item)?.is_empty() {
            return Err(StoreError::NoChanges(item.name.clone()));
        }
        let mut message = format!("Откат к {version}");
        if let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) {
            message.push_str(": ");
            message.push_str(note);
        }
        message.push_str(&format!(
            "\n\nСодержимое взято из {version} ({}).",
            target.short_commit()
        ));
        self.commit(item, &message)
    }

    /// Прочитать файл детали из версии. Файлы LFS возвращаются целиком.
    pub fn read_file(&self, item: &Item, version: &VersionInfo, rel: &str) -> Result<Vec<u8>> {
        let spec = format!("{}:{}/{}", version.commit, self.pathspec(item), rel);
        self.git.run(&["cat-file", "--filters", &spec])
    }

    /// Паспорт детали в версии.
    pub fn meta_at(&self, item: &Item, version: &VersionInfo) -> Result<PartMeta> {
        let bytes = self.read_file(item, version, META_FILE)?;
        let text = String::from_utf8_lossy(&bytes);
        Ok(schema::from_str(&text, Path::new(META_FILE))?)
    }

    /// Дерево построения в версии, если оно было.
    pub fn tree_at(&self, item: &Item, version: &VersionInfo) -> Result<Option<FeatureTree>> {
        let has_tree = self.files_at(item, version)?.iter().any(|f| f == TREE_FILE);
        if !has_tree {
            return Ok(None);
        }
        let bytes = self.read_file(item, version, TREE_FILE)?;
        let text = String::from_utf8_lossy(&bytes);
        Ok(Some(schema::from_str(&text, Path::new(TREE_FILE))?))
    }

    /// Файлы детали в версии (пути внутри папки детали).
    pub fn files_at(&self, item: &Item, version: &VersionInfo) -> Result<Vec<String>> {
        let path = self.pathspec(item);
        let out = self.git.run(&[
            "ls-tree",
            "-r",
            "-z",
            "--name-only",
            &version.commit,
            "--",
            &literal(&path),
        ])?;
        let prefix = format!("{path}/");
        Ok(String::from_utf8_lossy(&out)
            .split('\0')
            .filter(|f| !f.is_empty())
            .map(|f| f.strip_prefix(&prefix).unwrap_or(f).to_string())
            .collect())
    }

    /// Выгрузить все файлы версии в папку `dest`.
    pub fn export(&self, item: &Item, version: &VersionInfo, dest: &Path) -> Result<Vec<PathBuf>> {
        let mut written = Vec::new();
        for rel in self.files_at(item, version)? {
            let bytes = self.read_file(item, version, &rel)?;
            let out = rel
                .split('/')
                .fold(dest.to_path_buf(), |p, part| p.join(part));
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| StoreError::Io {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
            fs::write(&out, bytes).map_err(|e| StoreError::Io {
                path: out.clone(),
                source: e,
            })?;
            written.push(out);
        }
        Ok(written)
    }

    /// Папка с файлами версии. Файлы выкладываются один раз в кэш внутри
    /// `.git/nonius/versions/<коммит>` и дальше переиспользуются: версия не
    /// меняется, а `.git` не попадает в историю.
    pub fn version_dir(&self, item: &Item, version: &VersionInfo) -> Result<PathBuf> {
        let cache = self
            .project
            .root()
            .join(".git")
            .join("nonius")
            .join("versions");
        let dir = cache.join(&version.commit);
        let done = dir.join(".complete");
        if done.is_file() {
            return Ok(dir);
        }
        let tmp = cache.join(format!("{}.tmp", version.commit));
        if tmp.exists() {
            fs::remove_dir_all(&tmp).map_err(|e| StoreError::Io {
                path: tmp.clone(),
                source: e,
            })?;
        }
        fs::create_dir_all(&tmp).map_err(|e| StoreError::Io {
            path: tmp.clone(),
            source: e,
        })?;
        self.export(item, version, &tmp)?;
        fs::write(tmp.join(".complete"), b"").map_err(|e| StoreError::Io {
            path: tmp.clone(),
            source: e,
        })?;
        if dir.exists() {
            // Другой процесс успел раньше — его копия такая же.
            let _ = fs::remove_dir_all(&tmp);
            return Ok(dir);
        }
        fs::rename(&tmp, &dir).map_err(|e| StoreError::Io {
            path: dir.clone(),
            source: e,
        })?;
        Ok(dir)
    }

    /// Версия, как её видят модули сравнения: файлы версии лежат в `dir`.
    pub fn snapshot(&self, item: &Item, version: &VersionInfo) -> Result<Snapshot> {
        Ok(Snapshot {
            item: item.name.clone(),
            version: Some(version.version),
            dir: self.version_dir(item, version)?,
            meta: self.meta_at(item, version)?,
            tree: self.tree_at(item, version)?,
        })
    }

    /// Текущее (ещё не зафиксированное) состояние детали.
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
            meta: self.project.load_meta(item)?,
            tree,
        })
    }
}

fn commit_message(item: &PartName, version: VersionNumber, message: &str) -> String {
    let mut lines = message.lines();
    let subject = lines.next().unwrap_or_default().trim();
    let body = lines.collect::<Vec<_>>().join("\n");
    let mut text = format!("{item} {version}: {subject}\n");
    if !body.trim().is_empty() {
        text.push('\n');
        text.push_str(body.trim());
        text.push('\n');
    }
    text.push_str(&format!(
        "\n{ITEM_TRAILER}{item}\n{VERSION_TRAILER}{version}\n"
    ));
    text
}

fn parse_record(record: &str, item: &PartName) -> Option<VersionInfo> {
    let record = record.trim_start_matches('\n');
    let mut fields = record.splitn(4, '\u{1f}');
    let commit = fields.next()?.trim().to_string();
    let author = fields.next()?.to_string();
    let date = fields.next()?.to_string();
    let body = fields.next()?;

    let mut found_item = None;
    let mut found_version = None;
    let mut kept = Vec::new();
    for line in body.lines() {
        if let Some(v) = line.strip_prefix(ITEM_TRAILER) {
            found_item = Some(v.trim());
        } else if let Some(v) = line.strip_prefix(VERSION_TRAILER) {
            found_version = v.trim().parse::<VersionNumber>().ok();
        } else {
            kept.push(line);
        }
    }
    if found_item? != item.as_str() {
        return None;
    }
    let version = found_version?;

    let prefix = format!("{item} {version}: ");
    let first: Option<&str> = kept.first().copied();
    if let Some(rest) = first.and_then(|line| line.strip_prefix(prefix.as_str())) {
        kept[0] = rest;
    }
    let message = kept.join("\n").trim().to_string();
    Some(VersionInfo {
        item: item.clone(),
        version,
        commit,
        author,
        date,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_round_trip() {
        let item = PartName::new("вал 20 мм").unwrap();
        let v = VersionNumber::new(3).unwrap();
        let text = commit_message(&item, v, "Фаска 1×45\n\nПод подшипник 6204.");
        assert!(text.starts_with("вал 20 мм v3: Фаска 1×45\n"));
        let record = format!("abc123\u{1f}Владимир\u{1f}2026-10-26T12:00:00+03:00\u{1f}{text}");
        let info = parse_record(&record, &item).unwrap();
        assert_eq!(info.version, v);
        assert_eq!(info.author, "Владимир");
        assert_eq!(info.message, "Фаска 1×45\n\nПод подшипник 6204.");
        assert_eq!(info.summary(), "Фаска 1×45");

        let other = PartName::new("вал").unwrap();
        assert!(parse_record(&record, &other).is_none(), "чужая деталь");
        assert!(parse_record("abc\u{1f}a\u{1f}d\u{1f}просто коммит", &item).is_none());
    }
}
