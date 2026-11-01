//! Окно программы: `nonius` без команды (двойной щелчок) и `nonius view`.
//!
//! Здесь — мост от общего слоя `nonius-app` к окну (`AppSource`): окно
//! не знает про git и OpenCascade, а сравнения берёт из того же кэша, что
//! и команды.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};

use anyhow::{bail, Context, Result};
use nonius_app::{
    is_text_file, App, CompareOptions, DiffLineKind, FileChangeKind, Mass, MaterialChoice,
    MATERIALS,
};
use nonius_core::meta::ItemKind;
use nonius_core::VersionNumber;
use nonius_gui::source::{
    Comparison, DiffEntry, DiffKind, FileEntry, FileState, MassEntry, MaterialEntry, PartEntry,
    Pick, Progress, ProjectSource, VersionEntry,
};
use nonius_scene::Scene;

use crate::format::{parse_version, short_date};
use crate::{welcome, window_registry};

/// Проект для окна поверх `nonius-app`.
struct AppSource {
    app: App,
    /// Одно сравнение за раз: OpenCascade падает при параллельном чтении STEP,
    /// а человек может быстро переключить версии, пока идёт прошлое сравнение.
    busy: Mutex<()>,
    /// Запись (сохранить версию, назначить материал) — когда никто не читает
    /// папку детали и git: иначе git спотыкается о свой index.lock, а
    /// сравнение видит полусохранённое.
    files: RwLock<()>,
}

impl AppSource {
    fn open(path: &Path) -> Result<Box<dyn ProjectSource>> {
        Ok(Box::new(Self {
            app: App::open(path, window_registry())?,
            busy: Mutex::new(()),
            files: RwLock::new(()),
        }))
    }

    fn reading(&self) -> RwLockReadGuard<'_, ()> {
        self.files.read().unwrap_or_else(|e| e.into_inner())
    }

    fn writing(&self) -> RwLockWriteGuard<'_, ()> {
        self.files.write().unwrap_or_else(|e| e.into_inner())
    }
}

/// Ход сравнения → строка состояния окна; «Отменить» в окне → отмена расчёта.
struct Steps<'a>(&'a dyn Progress);

impl nonius_app::Progress for Steps<'_> {
    fn step(&self, what: &str) {
        self.0.step(&format!("{what}…"));
    }

    fn cancelled(&self) -> bool {
        self.0.cancelled()
    }
}

/// Паспорт → материал и масса для окна.
fn mass_entry(p: nonius_app::Passport) -> MassEntry {
    let no_mass = match &p.mass {
        Mass::Computed { .. } | Mass::FromCad { .. } => None,
        Mass::NoMaterial => Some("материал не задан".to_string()),
        Mass::NoDensity { material } => Some(format!(
            "плотность «{material}» неизвестна — выберите «Свой материал…» и введите её"
        )),
        Mass::NoVolume => Some("объём не измерен: в модели нет геометрии".to_string()),
    };
    MassEntry {
        material: p.meta.material.clone(),
        density_kg_m3: p.meta.density_kg_m3,
        mass_kg: p.mass.kg(),
        no_mass,
    }
}

fn version(p: Pick) -> Result<Option<VersionNumber>> {
    Ok(match p {
        Pick::Version(n) => Some(VersionNumber::new(n)?),
        Pick::Current => None,
    })
}

/// Пара версий для вкладки «Файлы»: одна и та же версия — без сравнения.
fn pair(before: Pick, after: Pick) -> Result<(Option<VersionNumber>, Option<VersionNumber>)> {
    let to = version(after)?;
    let from = if before == after {
        None
    } else {
        version(before)?
    };
    Ok((from, to))
}

impl ProjectSource for AppSource {
    fn title(&self) -> String {
        self.app.project().config().name.clone()
    }

    fn root(&self) -> &Path {
        self.app.project().root()
    }

    fn parts(&self) -> Result<Vec<PartEntry>> {
        let _files = self.reading();
        Ok(self
            .app
            .items()?
            .into_iter()
            .map(|s| PartEntry {
                name: s.item.name.to_string(),
                assembly: s.item.kind == ItemKind::Assembly,
                latest: s.latest.as_ref().map(|v| v.version.to_string()),
                dirty: !s.changes.is_empty(),
            })
            .collect())
    }

    fn versions(&self, part: &str) -> Result<Vec<VersionEntry>> {
        let _files = self.reading();
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
        progress: &dyn Progress,
    ) -> Result<Comparison> {
        let Some(from) = version(before)? else {
            bail!("«Было» — это сохранённая версия: сравнивать можно с ней");
        };
        let _one_at_a_time = match self.busy.try_lock() {
            Ok(g) => g,
            Err(TryLockError::Poisoned(e)) => e.into_inner(),
            // Прошлое (отменённое) сравнение доделывает свой шаг.
            Err(TryLockError::WouldBlock) => {
                progress.step("Жду, пока закончится прошлое сравнение…");
                self.busy.lock().unwrap_or_else(|e| e.into_inner())
            }
        };
        let _files = self.reading();
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
            Err(nonius_app::Error::Cancelled) => bail!("сравнение отменено"),
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

    fn changed_files(&self, part: &str) -> Result<Vec<String>> {
        let _files = self.reading();
        let item = self.app.item(part)?;
        Ok(self
            .app
            .changes(&item)?
            .into_iter()
            .map(|f| {
                let mark = match f.kind {
                    FileChangeKind::Added => "+",
                    FileChangeKind::Modified => "~",
                    FileChangeKind::Deleted => "−",
                };
                format!("{mark} {}", f.path)
            })
            .collect())
    }

    fn save_version(&self, part: &str, message: &str) -> Result<u32> {
        let _files = self.writing();
        Ok(self.app.save_version(part, message)?.version.get())
    }

    fn mass(&self, part: &str, pick: Pick) -> Result<MassEntry> {
        let _files = self.reading();
        Ok(mass_entry(self.app.passport(part, version(pick)?)?))
    }

    fn materials(&self) -> Vec<MaterialEntry> {
        MATERIALS
            .iter()
            .map(|m| MaterialEntry {
                name: m.name.to_string(),
                density_kg_m3: m.density_kg_m3,
            })
            .collect()
    }

    fn set_material(
        &self,
        part: &str,
        material: Option<&str>,
        density_kg_m3: Option<f64>,
    ) -> Result<MassEntry> {
        let choice = material.map(|name| MaterialChoice {
            name: name.to_string(),
            density_kg_m3,
        });
        let _files = self.writing();
        Ok(mass_entry(self.app.set_material(part, choice)?))
    }

    fn files(&self, part: &str, before: Pick, after: Pick) -> Result<Vec<FileEntry>> {
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

    fn file_diff(
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

    fn open_file(&self, part: &str, pick: Pick, path: &str) -> Result<PathBuf> {
        let _files = self.reading();
        Ok(self.app.open_file(part, version(pick)?, path)?)
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

    /// Ход работы для проверки: запоминает шаги, может «нажать Отменить».
    #[derive(Default)]
    struct Said {
        steps: Mutex<Vec<String>>,
        cancel: bool,
    }

    impl Progress for Said {
        fn step(&self, what: &str) {
            self.steps.lock().unwrap().push(what.to_string());
        }
        fn cancelled(&self) -> bool {
            self.cancel
        }
    }

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

        let said = Said::default();
        let c = src
            .compare("кронштейн", Pick::Version(1), Pick::Version(2), &said)
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
        assert!(
            !said.steps.lock().unwrap().is_empty(),
            "окно видит ход работы"
        );
        let quiet = Said::default();
        assert!(src
            .compare("нет такой", Pick::Version(1), Pick::Current, &quiet)
            .is_err());
        assert!(src
            .compare("кронштейн", Pick::Current, Pick::Version(1), &quiet)
            .is_err());
        // «Отменить» — ни сравнения, ни подмены одними числами.
        let cancelled = Said {
            cancel: true,
            ..Said::default()
        };
        let e = src
            .compare("кронштейн", Pick::Version(2), Pick::Current, &cancelled)
            .unwrap_err();
        assert!(format!("{e:#}").contains("отменен"), "{e:#}");
    }

    #[test]
    fn material_and_saving_from_the_window() {
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
        let step = d.join("пластина.step");
        std::fs::write(&step, STEP).unwrap();
        App::open(d, registry())
            .unwrap()
            .import(&step, None)
            .unwrap();
        let src = AppSource::open(d).unwrap();

        // Новая деталь без версий — есть что сохранить.
        assert!(src.parts().unwrap()[0].dirty);
        assert!(!src.changed_files("пластина").unwrap().is_empty());
        assert!(src.save_version("пластина", "  ").is_err(), "без описания");
        assert_eq!(src.save_version("пластина", "Первая").unwrap(), 1);
        assert!(!src.parts().unwrap()[0].dirty);

        let none = src.mass("пластина", Pick::Version(1)).unwrap();
        assert_eq!(none.material, None);
        assert_eq!(none.mass_kg, None);
        assert!(none.no_mass.unwrap().contains("материал не задан"));

        // Из справочника плотность берётся сама.
        let st3 = src.set_material("пластина", Some("Ст3"), None).unwrap();
        assert_eq!(st3.material.as_deref(), Some("Ст3"));
        assert_eq!(st3.density_kg_m3, Some(7850.0));
        assert!(src.materials().iter().any(|m| m.name == "Ст3"));
        let parts = src.parts().unwrap();
        assert!(parts[0].dirty, "материал меняет деталь");
        assert!(src
            .changed_files("пластина")
            .unwrap()
            .iter()
            .any(|f| f.contains("meta.yaml")));

        // Свой материал — с плотностью вручную.
        let own = src
            .set_material("пластина", Some("Сплав заказчика"), Some(2850.0))
            .unwrap();
        assert_eq!(own.density_kg_m3, Some(2850.0));
        assert_eq!(src.save_version("пластина", "Материал").unwrap(), 2);
        let v2 = src.mass("пластина", Pick::Version(2)).unwrap();
        assert_eq!(v2.material.as_deref(), Some("Сплав заказчика"));

        // Убрать материал — массы снова нет.
        let cleared = src.set_material("пластина", None, None).unwrap();
        assert_eq!(cleared.material, None);
        assert!(cleared.no_mass.is_some());
    }
}
