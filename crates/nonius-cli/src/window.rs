//! Окно программы: `nonius` без команды (двойной щелчок) и `nonius view`.
//!
//! Здесь — мост от общего слоя `nonius-app` к окну (`AppSource`): окно
//! не знает про git и OpenCascade, а сравнения берёт из того же кэша, что
//! и команды.

use std::path::Path;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use nonius_app::{App, CompareOptions, Progress};
use nonius_core::meta::ItemKind;
use nonius_core::VersionNumber;
use nonius_gui::source::{Comparison, PartEntry, Pick, ProjectSource, VersionEntry};
use nonius_scene::Scene;

use crate::format::{parse_version, short_date};
use crate::{welcome, window_registry};

/// Проект для окна поверх `nonius-app`.
struct AppSource {
    app: App,
    /// Одно сравнение за раз: OpenCascade падает при параллельном чтении STEP,
    /// а человек может быстро переключить версии, пока идёт прошлое сравнение.
    busy: Mutex<()>,
}

impl AppSource {
    fn open(path: &Path) -> Result<Box<dyn ProjectSource>> {
        Ok(Box::new(Self {
            app: App::open(path, window_registry())?,
            busy: Mutex::new(()),
        }))
    }
}

/// Ход сравнения → строка состояния окна.
struct Steps<'a>(&'a (dyn Fn(&str) + Sync));

impl Progress for Steps<'_> {
    fn step(&self, what: &str) {
        (self.0)(&format!("{what}…"));
    }
}

fn version(p: Pick) -> Result<Option<VersionNumber>> {
    Ok(match p {
        Pick::Version(n) => Some(VersionNumber::new(n)?),
        Pick::Current => None,
    })
}

impl ProjectSource for AppSource {
    fn title(&self) -> String {
        self.app.project().config().name.clone()
    }

    fn root(&self) -> &Path {
        self.app.project().root()
    }

    fn parts(&self) -> Result<Vec<PartEntry>> {
        Ok(self
            .app
            .items()?
            .into_iter()
            .map(|s| PartEntry {
                name: s.item.name.to_string(),
                assembly: s.item.kind == ItemKind::Assembly,
                latest: s.latest.as_ref().map(|v| v.version.to_string()),
                dirty: s.latest.is_some() && !s.changes.is_empty(),
            })
            .collect())
    }

    fn versions(&self, part: &str) -> Result<Vec<VersionEntry>> {
        let mut list: Vec<VersionEntry> = self
            .app
            .versions(part)?
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
        progress: &(dyn Fn(&str) + Sync),
    ) -> Result<Comparison> {
        let Some(from) = version(before)? else {
            bail!("«Было» — это сохранённая версия: сравнивать можно с ней");
        };
        let _one_at_a_time = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        let options = CompareOptions {
            reports: true,
            render: true,
        };
        let steps = Steps(progress);
        let to = version(after)?;
        let c = match self.app.compare(part, Some(from), to, &options, &steps) {
            Ok(c) => c,
            // 3D не получилось (нет OpenCascade, в STEP нет геометрии) —
            // числа всё равно покажем, а причину скажем словами.
            Err(e) => {
                let problem = match e {
                    nonius_app::Error::NoRenderer => None,
                    e => Some(format!("{e:#}")),
                };
                let numbers = CompareOptions::default();
                let c = self.app.compare(part, Some(from), to, &numbers, &steps)?;
                return Ok(Comparison {
                    scene: None,
                    scene_problem: problem,
                    reports: c.reports,
                });
            }
        };
        let (scene, scene_problem) = match c.file(nonius_occt::CHANGES_GLB) {
            None => (None, None),
            Some(glb) => match std::fs::read(glb)
                .map_err(anyhow::Error::from)
                .and_then(|b| Ok(Scene::from_glb(&b)?))
            {
                Ok(s) => (Some(s), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            },
        };
        Ok(Comparison {
            scene,
            scene_problem,
            reports: c.reports,
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
    let source = AppSource::open(&start).ok();
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
        opener: Arc::new(|p: &Path| AppSource::open(p)),
        part: part.map(str::to_string),
        picks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry;

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
        nonius_app::create_project(d, Some("Станок")).unwrap();
        let step = d.join("кронштейн.step");
        std::fs::write(&step, STEP).unwrap();
        let app = App::open(d, registry()).unwrap();
        app.import(&step, None).unwrap();
        app.save_version("кронштейн", "Первая").unwrap();
        let meta = d.join("parts").join("кронштейн").join("meta.yaml");
        let text = std::fs::read_to_string(&meta).unwrap() + "material: Ст3\n";
        std::fs::write(&meta, text).unwrap();
        app.save_version("кронштейн", "Материал").unwrap();

        // Из подпапки проекта — тоже находится.
        let src = AppSource::open(&d.join("parts")).unwrap();
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
            .compare("нет такой", Pick::Version(1), Pick::Current, &|_| {})
            .is_err());
        assert!(src
            .compare("кронштейн", Pick::Current, Pick::Version(1), &|_| {})
            .is_err());
    }
}
