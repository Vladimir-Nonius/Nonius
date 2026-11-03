//! Файлы детали для окна: список версии, разница текста, открыть файл.

use std::path::PathBuf;

use anyhow::Result;
use nonius_app::{is_text_file, DiffLineKind, FileChangeKind};
use nonius_gui::source::{DiffEntry, DiffKind, FileEntry, FileState, OpenWay, Pick, ProgramEntry};

use super::{pair, version, AppSource};

impl AppSource {
    pub(super) fn file_list(
        &self,
        part: &str,
        before: Pick,
        after: Pick,
    ) -> Result<Vec<FileEntry>> {
        let _files = self.reading();
        let (from, to) = pair(before, after)?;
        Ok(self
            .app
            .version_files(part, from, to)?
            .into_iter()
            .map(|f| FileEntry {
                text: is_text_file(&f.path),
                state: match f.change {
                    None => FileState::Same,
                    Some(FileChangeKind::Added) => FileState::Added,
                    Some(FileChangeKind::Modified) => FileState::Modified,
                    Some(FileChangeKind::Deleted) => FileState::Deleted,
                },
                before: f.before,
                after: f.after,
                path: f.path,
            })
            .collect())
    }

    pub(super) fn file_lines(
        &self,
        part: &str,
        before: Pick,
        after: Pick,
        path: &str,
    ) -> Result<Vec<DiffEntry>> {
        let _files = self.reading();
        let (from, to) = pair(before, after)?;
        Ok(self
            .app
            .file_diff(part, from, to, path)?
            .into_iter()
            .map(|l| DiffEntry {
                kind: match l.kind {
                    DiffLineKind::Same => DiffKind::Same,
                    DiffLineKind::Added => DiffKind::Added,
                    DiffLineKind::Removed => DiffKind::Removed,
                    DiffLineKind::Gap => DiffKind::Gap,
                },
                text: l.text,
            })
            .collect())
    }

    pub(super) fn open_with(
        &self,
        part: &str,
        pick: Pick,
        path: &str,
        way: &OpenWay,
    ) -> Result<PathBuf> {
        let way = match way {
            OpenWay::Default => nonius_app::OpenWay::Default,
            OpenWay::Program(p) => nonius_app::OpenWay::Program(nonius_app::Program {
                name: p.name.clone(),
                command: p.command.clone(),
            }),
            OpenWay::Choose => nonius_app::OpenWay::Choose,
            OpenWay::Folder => nonius_app::OpenWay::Folder,
        };
        let _files = self.reading();
        Ok(self.app.open_file(part, version(pick)?, path, &way)?)
    }
}

/// Программы, которые система знает для такого файла.
pub(super) fn programs(path: &str) -> Vec<ProgramEntry> {
    nonius_app::programs_for(path)
        .into_iter()
        .map(|p| ProgramEntry {
            name: p.name,
            command: p.command,
        })
        .collect()
}
