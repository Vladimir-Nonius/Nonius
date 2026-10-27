//! Окно программы: `nonius` без команды (двойной щелчок) и `nonius view`.
//!
//! Здесь — временный мост от хранилища к окну (`StoreSource`). Когда общий
//! слой `nonius-app` попадёт в main, окно будет брать проект оттуда.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use nonius_core::compare::{ChangeReport, CompareContext};
use nonius_core::extension::{Registry, Snapshot};
use nonius_core::meta::ItemKind;
use nonius_core::{PartName, Project, VersionNumber};
use nonius_gui::source::{Comparison, PartEntry, Pick, ProjectSource, VersionEntry};
use nonius_scene::Scene;
use nonius_store::{GitOptions, Store};

use crate::format::{parse_version, short_date};
use crate::{registry, welcome};

/// Проект для окна поверх хранилища git.
struct StoreSource {
    store: Store,
}

impl StoreSource {
    fn open(path: &Path) -> Result<Box<dyn ProjectSource>> {
        let project = Project::discover(path)?;
        let store = Store::open(project, &GitOptions::default())?;
        Ok(Box::new(Self { store }))
    }

    fn snapshot(&self, part: &str, pick: Pick) -> Result<Snapshot> {
        let item = self.store.item(part)?;
        Ok(match pick {
            Pick::Current => self.store.working_snapshot(&item)?,
            Pick::Version(n) => {
                let info = self.store.version(&item, VersionNumber::new(n)?)?;
                self.store.snapshot(&item, &info)?
            }
        })
    }

    /// Сцена изменений от модуля `scene-occt`; `None` — модуля нет или нет STEP.
    /// Сравнение двух сохранённых версий не меняется — берётся из `out`, если уже есть.
    fn scene(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        out: &Path,
        cached: bool,
    ) -> Result<Option<Scene>> {
        let saved = out.join(nonius_occt::CHANGES_GLB);
        if cached {
            if let Some(scene) = std::fs::read(&saved)
                .ok()
                .and_then(|b| Scene::from_glb(&b).ok())
            {
                return Ok(Some(scene));
            }
        }
        let reg = registry();
        let Some(renderer) = reg
            .renderers()
            .find(|r| r.manifest().id == nonius_occt::SCENE_RENDERER)
        else {
            return Ok(None);
        };
        let files = renderer.render(before, after, &ChangeReport::empty(""), out)?;
        let Some(glb) = files.iter().find(|f| f.ends_with(nonius_occt::CHANGES_GLB)) else {
            return Ok(None);
        };
        let bytes = std::fs::read(glb)?;
        Ok(Some(Scene::from_glb(&bytes)?))
    }
}

fn label(p: Pick) -> String {
    match p {
        Pick::Version(n) => format!("v{n}"),
        Pick::Current => "current".into(),
    }
}

impl ProjectSource for StoreSource {
    fn title(&self) -> String {
        self.store.project().config().name.clone()
    }

    fn root(&self) -> &Path {
        self.store.project().root()
    }

    fn parts(&self) -> Result<Vec<PartEntry>> {
        let mut out = Vec::new();
        for item in self.store.project().items()? {
            out.push(PartEntry {
                name: item.name.to_string(),
                assembly: item.kind == ItemKind::Assembly,
                latest: self.store.latest(&item)?.map(|v| v.version.to_string()),
                dirty: !self.store.changes(&item)?.is_empty(),
            });
        }
        Ok(out)
    }

    fn versions(&self, part: &str) -> Result<Vec<VersionEntry>> {
        let item = self.store.item(part)?;
        let mut list: Vec<VersionEntry> = self
            .store
            .versions(&item)?
            .iter()
            .map(|v| VersionEntry {
                number: v.version.get(),
                date: short_date(&v.date),
                author: v.author.clone(),
                summary: v.summary().to_string(),
            })
            .collect();
        list.sort_by_key(|v| std::cmp::Reverse(v.number));
        Ok(list)
    }

    fn compare(
        &self,
        part: &str,
        before: Pick,
        after: Pick,
        progress: &dyn Fn(&str),
    ) -> Result<Comparison> {
        progress("Читаю версии…");
        let a = self.snapshot(part, before)?;
        let b = self.snapshot(part, after)?;
        // Числа из паспортов (быстро). Форму сравнивает сцена ниже.
        let ctx = CompareContext {
            tolerance_mm: self.store.project().config().compare.tolerance_mm,
            ..CompareContext::default()
        };
        let reports = Registry::with_builtins().compare_all(&a, &b, &ctx)?;
        progress("Сравниваю форму — у больших деталей до минуты…");
        let name = PartName::new(part)?;
        let out = self
            .root()
            .join(".git")
            .join("nonius")
            .join("window")
            .join(name.as_str())
            .join(format!("{}-{}", label(before), label(after)));
        let both_saved = matches!((before, after), (Pick::Version(_), Pick::Version(_)));
        // Без 3D числа всё равно полезны: ошибку сцены показываем словами.
        let (scene, scene_problem) = match self.scene(&a, &b, &out, both_saved) {
            Ok(scene) => (scene, None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        Ok(Comparison {
            scene,
            scene_problem,
            reports,
        })
    }
}

/// Запуск без команды. В Windows окно запускается отдельным процессом без
/// чёрного окна консоли, а этот процесс сразу выходит. `NONIUS_NO_WINDOW=1`
/// (серверы, проверки) — только подсказка в консоли.
pub fn double_click() -> ExitCode {
    if std::env::var_os("NONIUS_NO_WINDOW").is_some_and(|v| v == "1") {
        welcome::welcome();
        return ExitCode::SUCCESS;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if let Ok(exe) = std::env::current_exe() {
            let child = std::process::Command::new(exe)
                .arg("view")
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
            if child.is_ok() {
                return ExitCode::SUCCESS;
            }
        }
    }
    match open(None, None, None, None) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ошибка: {e:#}\n");
            welcome::welcome();
            ExitCode::FAILURE
        }
    }
}

/// `nonius view`: открыть окно, сразу показать деталь и версии, если заданы.
pub fn open(
    project: Option<&Path>,
    part: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<()> {
    let start = match project {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir().context("не удалось узнать текущую папку")?,
    };
    // Не в папке проекта — окно откроет последний проект или начальный экран.
    let source = StoreSource::open(&start).ok();
    let pick = |v: Option<&str>| -> Result<Option<Pick>> {
        v.map(|v| Ok(Pick::Version(parse_version(v)?.get())))
            .transpose()
    };
    let picks = match (pick(from)?, pick(to)?) {
        (None, None) => None,
        (a, b) => Some((a.unwrap_or(Pick::Current), b.unwrap_or(Pick::Current))),
    };
    nonius_gui::run(nonius_gui::Options {
        project: source,
        opener: Arc::new(|p: &Path| StoreSource::open(&PathBuf::from(p))),
        part: part.map(str::to_string),
        picks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{history, import, project};

    const STEP: &str = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\nFILE_NAME('кронштейн','2026-10-26',('Владимир'),(''),'SwSTEP 2.0','SolidWorks 2020','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n";

    #[test]
    fn store_feeds_the_window() {
        for (k, v) in [
            ("GIT_AUTHOR_NAME", "Тестировщик"),
            ("GIT_AUTHOR_EMAIL", "test@example.com"),
            ("GIT_COMMITTER_NAME", "Тестировщик"),
            ("GIT_COMMITTER_EMAIL", "test@example.com"),
        ] {
            std::env::set_var(k, v);
        }
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        project::init(Some(&d.to_path_buf()), Some(&"Станок".to_string())).unwrap();
        let step = d.join("кронштейн.step");
        std::fs::write(&step, STEP).unwrap();
        import::import(&Project::discover(d).unwrap(), &step, None).unwrap();
        let store = || Store::open(Project::discover(d).unwrap(), &GitOptions::default()).unwrap();
        history::commit(&store(), "кронштейн", "Первая").unwrap();
        let meta = d.join("parts").join("кронштейн").join("meta.yaml");
        let text = std::fs::read_to_string(&meta).unwrap() + "material: Ст3\n";
        std::fs::write(&meta, text).unwrap();
        history::commit(&store(), "кронштейн", "Материал").unwrap();

        // Из подпапки проекта — тоже находится.
        let src = StoreSource::open(&d.join("parts")).unwrap();
        assert_eq!(src.title(), "Станок");
        let parts = src.parts().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].name, "кронштейн");
        assert_eq!(parts[0].latest.as_deref(), Some("v2"));
        assert!(!parts[0].dirty);

        let versions = src.versions("кронштейн").unwrap();
        let numbers: Vec<_> = versions.iter().map(|v| v.number).collect();
        assert_eq!(numbers, [2, 1], "новые первыми");
        assert_eq!(versions[0].summary, "Материал");
        assert_eq!(versions[0].author, "Тестировщик");

        let said = std::sync::Mutex::new(Vec::new());
        let c = src
            .compare(
                "кронштейн",
                Pick::Version(1),
                Pick::Version(2),
                &|m| said.lock().unwrap().push(m.to_string()),
            )
            .unwrap();
        let lines: Vec<String> = c
            .reports
            .iter()
            .flat_map(|r| &r.changes)
            .map(|ch| ch.to_string())
            .collect();
        assert!(lines.iter().any(|l| l.contains("Материал")), "{lines:?}");
        // В STEP только заголовок — показывать в 3D нечего, а числа есть.
        assert!(c.scene.is_none());
        if let Some(p) = &c.scene_problem {
            assert!(p.contains("нет геометрии"), "{p}");
        }
        assert!(!said.lock().unwrap().is_empty(), "окно видит ход работы");
        assert!(src
            .compare("нет такой", Pick::Current, Pick::Current, &|_| {})
            .is_err());
    }
}
