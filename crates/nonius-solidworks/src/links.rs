//! Связи файлов SolidWorks: из чего собрана сборка, к какой модели чертёж.
//!
//! SolidWorks отвечает по файлам на диске, не открывая их: вся папка — один
//! запуск скрипта `links.ps1` и доли секунды.

use std::path::{Path, PathBuf};

use nonius_core::extension::{ExtensionPoint, LinkSource, Module, ModuleManifest, API_VERSION};
use nonius_core::{Error, Result};

#[cfg(any(windows, test))]
const LINKS: &str = include_str!("links.ps1");

/// Модуль: связи файлов `.sldasm`, `.slddrw`, `.sldprt` от установленного
/// SolidWorks.
pub struct SolidWorksLinks {
    manifest: ModuleManifest,
}

impl SolidWorksLinks {
    /// Идентификатор модуля.
    pub const ID: &'static str = "solidworks-links";

    /// Создать модуль.
    pub fn new() -> Self {
        Self {
            manifest: ModuleManifest {
                id: Self::ID.into(),
                name: "Связи файлов SolidWorks".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                api: API_VERSION,
                points: vec![ExtensionPoint::Links],
                permissions: vec![],
            },
        }
    }
}

impl Default for SolidWorksLinks {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for SolidWorksLinks {
    fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
}

impl LinkSource for SolidWorksLinks {
    fn knows(&self, path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            ["sldasm", "slddrw", "sldprt"]
                .iter()
                .any(|known| e.eq_ignore_ascii_case(known))
        })
    }

    fn links(&self, files: &[PathBuf]) -> Result<Vec<Vec<PathBuf>>> {
        ask(files).map_err(|message| Error::Module {
            module: Self::ID.into(),
            message,
        })
    }
}

/// Спросить SolidWorks про все файлы одним запуском.
fn ask(files: &[PathBuf]) -> std::result::Result<Vec<Vec<PathBuf>>, String> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(windows)]
    {
        let dir = tempfile::tempdir().map_err(|e| format!("временная папка: {e}"))?;
        let list = dir.path().join("files.txt");
        let text: String = files
            .iter()
            .map(|f| format!("{}\r\n", f.display()))
            .collect();
        std::fs::write(&list, text).map_err(|e| format!("список файлов: {e}"))?;
        let lines = crate::run::script(LINKS, &[("-List", &list)])?;
        Ok(parse(&lines, files))
    }
    #[cfg(not(windows))]
    {
        Err("SolidWorks есть только в Windows".into())
    }
}

/// Разобрать ответ скрипта: `FILE<таб>путь`, за ним его `DEP<таб>путь`.
/// Ответ раскладывается по порядку вопроса; о ком скрипт промолчал — пусто.
#[cfg(any(windows, test))]
fn parse(lines: &[String], files: &[PathBuf]) -> Vec<Vec<PathBuf>> {
    let mut found = vec![Vec::new(); files.len()];
    let mut next = 0;
    let mut current = None;
    for line in lines {
        if line.strip_prefix("FILE\t").is_some() {
            // Строки FILE идут в порядке вопроса, по одной на файл.
            current = (next < files.len()).then_some(next);
            next += 1;
        } else if let (Some(dep), Some(i)) = (line.strip_prefix("DEP\t"), current) {
            let dep = PathBuf::from(dep.trim());
            if !found[i].contains(&dep) {
                found[i].push(dep);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_is_laid_out_in_question_order() {
        let files = [
            PathBuf::from(r"C:\п\узел.SLDASM"),
            PathBuf::from(r"C:\п\вал.SLDPRT"),
            PathBuf::from(r"C:\п\вал.SLDDRW"),
        ];
        let lines: Vec<String> = [
            "FILE\tC:\\п\\узел.SLDASM",
            "DEP\tC:\\п\\вал.SLDPRT",
            "DEP\tC:\\п\\втулка.SLDPRT",
            "DEP\tC:\\п\\вал.SLDPRT",
            "FILE\tC:\\п\\вал.SLDPRT",
            "FILE\tC:\\п\\вал.SLDDRW",
            "DEP\tC:\\п\\вал.SLDPRT",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let links = parse(&lines, &files);
        assert_eq!(links[0].len(), 2, "повтор детали в сборке — одна связь");
        assert!(links[1].is_empty());
        assert_eq!(links[2], [PathBuf::from(r"C:\п\вал.SLDPRT")]);
    }

    #[test]
    fn script_answers_in_the_agreed_words() {
        assert!(LINKS.contains("\"FILE`t$file\""));
        assert!(LINKS.contains("\"DEP`t$($pairs[$i])\""));
        assert!(!LINKS.contains("ExitApp"));
    }
}
