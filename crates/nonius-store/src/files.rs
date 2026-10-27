//! Файлы версии: чтение из git, выгрузка, кэш версий и снимки для модулей.

use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::extension::Snapshot;
use nonius_core::meta::PartMeta;
use nonius_core::project::{Item, META_FILE, TREE_FILE};
use nonius_core::schema;
use nonius_core::tree::FeatureTree;

use crate::git::literal;
use crate::{workdir, Result, Store, StoreError, VersionInfo};

impl Store {
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
        let dir = workdir::work_dir(self.project.root())
            .join("versions")
            .join(&version.commit);
        let ready = |d: &Path| d.join(".complete").is_file();
        if ready(&dir) {
            return Ok(dir);
        }
        let io = |e| StoreError::Io {
            path: dir.clone(),
            source: e,
        };
        let tmp = workdir::tmp_dir_for(&dir).map_err(io)?;
        let filled = self.export(item, version, &tmp).and_then(|_| {
            fs::write(tmp.join(".complete"), b"").map_err(|e| StoreError::Io {
                path: tmp.clone(),
                source: e,
            })
        });
        if let Err(e) = filled {
            let _ = fs::remove_dir_all(&tmp);
            return Err(e);
        }
        workdir::publish_dir(&tmp, &dir, ready).map_err(io)?;
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
        Ok(self.project.working_snapshot(item)?)
    }
}
