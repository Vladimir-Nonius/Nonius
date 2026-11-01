//! Разница между версиями по файлам: что добавлено, изменено, удалено, какого
//! размера и какие строки поменялись в текстовых файлах. Версия `None` —
//! то, что сейчас лежит в папке детали.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use nonius_core::project::Item;

use crate::git::literal;
use crate::{
    DiffLine, DiffLineKind, FileChangeKind, Result, Store, StoreError, VersionFile, VersionInfo,
};

/// Указатель LFS меньше этого размера (так в описании формата).
const LFS_POINTER_MAX: u64 = 1024;

/// Текстовый ли файл: для них показываем разницу по строкам. Остальное
/// (PDF, DXF, STEP, PowerMill) — только «изменён» и размер.
pub fn is_text_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_lowercase());
    matches!(extension.as_deref(), Some("md" | "txt" | "yaml" | "yml"))
}

impl Store {
    /// Файлы версии `to` и всё, что изменилось после `from`, по порядку путей.
    /// Без `from` список просто показывает `to`, изменений в нём нет.
    pub fn files_between(
        &self,
        item: &Item,
        from: Option<&VersionInfo>,
        to: Option<&VersionInfo>,
    ) -> Result<Vec<VersionFile>> {
        let after = self.sizes(item, to)?;
        let before = match from {
            Some(v) => self.sizes(item, Some(v))?,
            None => BTreeMap::new(),
        };
        let changed = match from {
            Some(v) => self.changed_between(item, v, to)?,
            None => BTreeMap::new(),
        };
        let paths: BTreeSet<&String> = after.keys().chain(before.keys()).collect();
        Ok(paths
            .into_iter()
            .map(|path| VersionFile {
                path: path.clone(),
                change: changed.get(path).copied(),
                before: before.get(path).copied(),
                after: after.get(path).copied(),
            })
            .collect())
    }

    /// Строки текстового файла: разница между версиями, а без `from` — весь
    /// файл версии `to`.
    pub fn text_diff(
        &self,
        item: &Item,
        from: Option<&VersionInfo>,
        to: Option<&VersionInfo>,
        rel: &str,
    ) -> Result<Vec<DiffLine>> {
        let Some(from) = from else {
            return self.whole_text(item, to, rel, DiffLineKind::Same);
        };
        if self.moved_between(item, from, to) {
            return self.moved_text_diff(item, from, to, rel);
        }
        let file = literal(&format!("{}/{rel}", self.folder_at(item, from)));
        let mut args = vec!["diff", "--no-color", "--no-ext-diff", "--no-renames"];
        args.push(&from.commit);
        if let Some(to) = to {
            args.push(&to.commit);
        }
        args.extend(["--", &file]);
        let out = self.git.run(&args)?;
        // Новый файл, который git ещё не видел, в разницу не попадает.
        if out.is_empty() && to.is_none() && !self.files_at(item, from)?.iter().any(|f| f == rel) {
            return self.whole_text(item, None, rel, DiffLineKind::Added);
        }
        Ok(parse_diff(&String::from_utf8_lossy(&out)))
    }

    /// Деталь переименовали между этими версиями: пути в них разные.
    fn moved_between(&self, item: &Item, from: &VersionInfo, to: Option<&VersionInfo>) -> bool {
        let end = to.map_or_else(|| self.pathspec(item), |v| self.folder_at(item, v).into());
        self.folder_at(item, from) != end
    }

    /// Разница текста через переименование.
    fn moved_text_diff(
        &self,
        item: &Item,
        from: &VersionInfo,
        to: Option<&VersionInfo>,
        rel: &str,
    ) -> Result<Vec<DiffLine>> {
        let was = self.files_at(item, from)?.iter().any(|f| f == rel);
        let is = match to {
            Some(v) => self.files_at(item, v)?.iter().any(|f| f == rel),
            None => rel
                .split('/')
                .fold(item.dir.clone(), |p, s| p.join(s))
                .is_file(),
        };
        match (was, is) {
            (false, false) => return Ok(Vec::new()),
            (false, true) => return self.whole_text(item, to, rel, DiffLineKind::Added),
            (true, false) => return self.whole_text(item, Some(from), rel, DiffLineKind::Removed),
            (true, true) => {}
        }
        let old = format!("{}:{}/{rel}", from.commit, self.folder_at(item, from));
        let new = match to {
            Some(v) => format!("{}:{}/{rel}", v.commit, self.folder_at(item, v)),
            None => format!("{}/{rel}", self.pathspec(item)),
        };
        let mut args = vec!["diff", "--no-color", "--no-ext-diff", &old];
        if to.is_none() {
            args.push("--");
        }
        args.push(&new);
        let out = self.git.run(&args)?;
        Ok(parse_diff(&String::from_utf8_lossy(&out)))
    }

    fn whole_text(
        &self,
        item: &Item,
        version: Option<&VersionInfo>,
        rel: &str,
        kind: DiffLineKind,
    ) -> Result<Vec<DiffLine>> {
        let bytes = match version {
            Some(v) => self.read_file(item, v, rel)?,
            None => {
                let path = rel.split('/').fold(item.dir.clone(), |p, s| p.join(s));
                fs::read(&path).map_err(|e| StoreError::Io { path, source: e })?
            }
        };
        Ok(String::from_utf8_lossy(&bytes)
            .lines()
            .map(|text| DiffLine {
                kind,
                text: text.to_string(),
            })
            .collect())
    }

    /// Размеры файлов версии или папки детали сейчас.
    fn sizes(&self, item: &Item, version: Option<&VersionInfo>) -> Result<BTreeMap<String, u64>> {
        let path = match version {
            Some(v) => self.folder_at(item, v).into_owned(),
            None => self.pathspec(item),
        };
        let prefix = format!("{path}/");
        let rel = |f: &str| f.strip_prefix(&prefix).unwrap_or(f).to_string();
        let mut sizes = BTreeMap::new();
        match version {
            Some(v) => {
                let out = self.git.run(&[
                    "ls-tree",
                    "-r",
                    "-l",
                    "-z",
                    &v.commit,
                    "--",
                    &literal(&path),
                ])?;
                let text = String::from_utf8_lossy(&out);
                for (head, file) in text.split('\0').filter_map(|e| e.split_once('\t')) {
                    let mut fields = head.split_whitespace().skip(2);
                    let (Some(blob), Some(size)) = (fields.next(), fields.next()) else {
                        continue;
                    };
                    sizes.insert(rel(file), self.real_size(blob, size.parse().unwrap_or(0))?);
                }
            }
            None => {
                let out = self.git.run(&[
                    "ls-files",
                    "-z",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                    "--",
                    &literal(&path),
                ])?;
                let root = self.project.root();
                for file in String::from_utf8_lossy(&out).split('\0') {
                    let full = file.split('/').fold(root.to_path_buf(), |p, s| p.join(s));
                    // В индексе есть, а с диска файл уже убрали.
                    if let (false, Ok(m)) = (file.is_empty(), fs::metadata(&full)) {
                        sizes.insert(rel(file), m.len());
                    }
                }
            }
        }
        Ok(sizes)
    }

    /// Настоящий размер файла: в git у файла LFS лежит только короткий указатель.
    fn real_size(&self, blob: &str, stored: u64) -> Result<u64> {
        if stored >= LFS_POINTER_MAX {
            return Ok(stored);
        }
        let bytes = self.git.run(&["cat-file", "blob", blob])?;
        Ok(lfs_size(&bytes).unwrap_or(stored))
    }

    fn changed_between(
        &self,
        item: &Item,
        from: &VersionInfo,
        to: Option<&VersionInfo>,
    ) -> Result<BTreeMap<String, FileChangeKind>> {
        if self.moved_between(item, from, to) {
            return self.changed_through_rename(item, from, to);
        }
        let path = self.folder_at(item, from);
        let spec = literal(&path);
        let mut args = vec!["diff", "--name-status", "--no-renames", "-z"];
        args.push(&from.commit);
        if let Some(to) = to {
            args.push(&to.commit);
        }
        args.extend(["--", &spec]);
        let out = self.git.run(&args)?;
        let prefix = format!("{path}/");
        let rel = |f: &str| f.strip_prefix(&prefix).unwrap_or(f).to_string();
        let mut changed = parse_name_status(&out, rel);
        if to.is_none() {
            let out = self.git.run(&[
                "ls-files",
                "-z",
                "--others",
                "--exclude-standard",
                "--",
                &spec,
            ])?;
            for file in String::from_utf8_lossy(&out).split('\0') {
                if !file.is_empty() {
                    changed.insert(rel(file), FileChangeKind::Added);
                }
            }
        }
        Ok(changed)
    }

    /// Что изменилось через переименование. Деревья двух папок сравниваются
    /// напрямую, а текущее состояние — с последней версией, поверх этого.
    fn changed_through_rename(
        &self,
        item: &Item,
        from: &VersionInfo,
        to: Option<&VersionInfo>,
    ) -> Result<BTreeMap<String, FileChangeKind>> {
        let old = format!("{}:{}", from.commit, self.folder_at(item, from));
        let new = match to {
            Some(v) => format!("{}:{}", v.commit, self.folder_at(item, v)),
            None => format!("HEAD:{}", self.pathspec(item)),
        };
        let args = ["diff", "--name-status", "--no-renames", "-z", &old, &new];
        let mut changed = parse_name_status(&self.git.run(&args)?, str::to_string);
        if to.is_some() {
            return Ok(changed);
        }
        for now in self.changes(item)? {
            use FileChangeKind::{Added, Deleted, Modified};
            match (changed.get(&now.path).copied(), now.kind) {
                (Some(Added), Deleted) => {
                    changed.remove(&now.path);
                }
                (Some(Added), _) => {}
                (Some(Deleted), Added) => {
                    changed.insert(now.path, Modified);
                }
                (_, kind) => {
                    changed.insert(now.path, kind);
                }
            }
        }
        Ok(changed)
    }
}

/// Вывод `git diff --name-status -z` → путь и вид изменения.
fn parse_name_status(out: &[u8], rel: impl Fn(&str) -> String) -> BTreeMap<String, FileChangeKind> {
    let text = String::from_utf8_lossy(out);
    let mut changed = BTreeMap::new();
    let mut fields = text.split('\0');
    while let (Some(status), Some(file)) = (fields.next(), fields.next()) {
        let kind = match status {
            "A" => FileChangeKind::Added,
            "D" => FileChangeKind::Deleted,
            _ => FileChangeKind::Modified,
        };
        changed.insert(rel(file), kind);
    }
    changed
}

/// Размер из указателя LFS.
fn lfs_size(bytes: &[u8]) -> Option<u64> {
    let text = std::str::from_utf8(bytes).ok()?;
    if !text.starts_with("version https://git-lfs") {
        return None;
    }
    text.lines()
        .find_map(|l| l.strip_prefix("size "))?
        .trim()
        .parse()
        .ok()
}

/// Вывод `git diff` → строки. Шапка отбрасывается, разрыв между кусками файла
/// становится отдельной строкой.
fn parse_diff(text: &str) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut started = false;
    for line in text.lines() {
        if line.starts_with("@@") {
            if started {
                lines.push(DiffLine {
                    kind: DiffLineKind::Gap,
                    text: String::new(),
                });
            }
            started = true;
            continue;
        }
        if !started {
            continue;
        }
        let (kind, rest) = match line.split_at_checked(1) {
            Some(("+", rest)) => (DiffLineKind::Added, rest),
            Some(("-", rest)) => (DiffLineKind::Removed, rest),
            Some((" ", rest)) => (DiffLineKind::Same, rest),
            // «\ No newline at end of file».
            Some(("\\", _)) => continue,
            _ => (DiffLineKind::Same, line),
        };
        lines.push(DiffLine {
            kind,
            text: rest.to_string(),
        });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_files() {
        assert!(is_text_file("docs/заметка.MD"));
        assert!(is_text_file("meta.yaml"));
        assert!(!is_text_file("docs/чертёж.pdf"));
        assert!(!is_text_file("model.step"));
        assert!(!is_text_file("md"));
    }

    #[test]
    fn pointer_size() {
        let p = b"version https://git-lfs.github.com/spec/v1\noid sha256:ab\nsize 12345\n";
        assert_eq!(lfs_size(p), Some(12345));
        assert_eq!(lfs_size(b"size 5\n"), None);
    }

    #[test]
    fn diff_lines() {
        let text = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n было\n-старое\n+новое\n\\ No newline at end of file\n@@ -9 +9 @@\n-а\n+б\n";
        let kinds: Vec<_> = parse_diff(text).iter().map(|l| l.kind).collect();
        use DiffLineKind::*;
        assert_eq!(kinds, [Same, Removed, Added, Gap, Removed, Added]);
        assert_eq!(parse_diff(text)[2].text, "новое");
    }
}
