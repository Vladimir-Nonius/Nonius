//! Файлы детали для окна: список версии, разница текста, открыть файл.

use std::path::{Path, PathBuf};

use anyhow::Result;
use nonius_app::{is_text_file, DiffLineKind, FileChangeKind};
use nonius_gui::source::{DiffEntry, DiffKind, FileEntry, FileState, OpenWay, Pick, ProgramEntry};

use super::{in_words, pair, version, AppSource};

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
        let way = system_way(way);
        let _files = self.reading();
        Ok(self.app.open_file(part, version(pick)?, path, &way)?)
    }

    /// Открыть файл из папки-группы.
    pub(super) fn open_loose(&self, path: &str, way: &OpenWay) -> Result<PathBuf> {
        let way = system_way(way);
        let _files = self.reading();
        Ok(self.app.open_folder_file(path, &way)?)
    }

    /// Копия своего файла — в папку-группу и сразу в историю.
    pub(super) fn add_loose(&self, folder: &str, file: &Path) -> Result<String> {
        let _files = self.writing();
        self.app
            .add_folder_file(Path::new(folder), file)
            .map_err(in_words)
    }
}

/// Чем открыть — словами `nonius-app`.
fn system_way(way: &OpenWay) -> nonius_app::OpenWay {
    match way {
        OpenWay::Default => nonius_app::OpenWay::Default,
        OpenWay::Program(p) => nonius_app::OpenWay::Program(nonius_app::Program {
            name: p.name.clone(),
            command: p.command.clone(),
        }),
        OpenWay::Choose => nonius_app::OpenWay::Choose,
        OpenWay::Folder => nonius_app::OpenWay::Folder,
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
