//! Окно программы: `nonius` без команды (двойной щелчок) и `nonius view`.
//!
//! Здесь — мост от общего слоя `nonius-app` к окну (`AppSource`): окно
//! не знает про git и OpenCascade, а сравнения берёт из того же кэша, что
//! и команды.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};

use crate::format::{parse_version, short_date};
use crate::window_lock::WindowLock;
use crate::{welcome, window_registry};

use anyhow::{bail, Context, Result};
use nonius_app::{
    is_text_file, App, CompareOptions, DiffLineKind, FileChangeKind, History, Mass, MaterialChoice,
    MATERIALS,
};
use nonius_core::meta::ItemKind;
use nonius_core::VersionNumber;
use nonius_gui::source::{
    AddedModel, Comparison, DiffEntry, DiffKind, FileEntry, FileState, Machine, MassEntry,
    MaterialEntry, OpenWay, PartEntry, Pick, ProgramEntry, Progress, ProjectSource, Readiness,
    VersionEntry,
};
use nonius_scene::Scene;

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
    /// Держит проект за этим процессом, пока он открыт.
    _lock: WindowLock,
}

impl AppSource {
    fn open(path: &Path) -> Result<Box<dyn ProjectSource>> {
        let app = App::open(path, window_registry())?;
        let lock = WindowLock::take(app.project().root())?;
        Ok(Box::new(Self {
            app,
            busy: Mutex::new(()),
            files: RwLock::new(()),
            _lock: lock,
        }))
    }

    /// Создать проект в папке, название — имя папки. Проект без истории
    /// версий окну не нужен: сохранить в нём было бы нельзя.
    fn create(path: &Path) -> Result<Box<dyn ProjectSource>> {
        let created = nonius_app::create_project(path, None)?;
        if let History::Off(why) = created.history {
            bail!("проект создан, но история версий не включена: {why}");
        }
        Self::open(path)
    }

    /// Деталь как она есть. Сцены нет — вторым значением идёт причина.
    fn model_scene(
        &self,
        part: &str,
        pick: Pick,
        steps: &Steps,
    ) -> (Option<Scene>, Option<String>) {
        let files = version(pick).and_then(|at| Ok(self.app.model_view(part, at, steps)?));
        match files {
            Ok(files) => read_scene(&files),
            Err(e) => match e.downcast_ref() {
                Some(nonius_app::Error::NoRenderer) => (None, None),
                _ => (None, Some(format!("{e:#}"))),
            },
        }
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

/// Прочитать сцену из файлов отрисовщиков.
fn read_scene(files: &[PathBuf]) -> (Option<Scene>, Option<String>) {
    let glb = files
        .iter()
        .find(|p| p.file_name().is_some_and(|n| n == nonius_occt::CHANGES_GLB));
    let Some(glb) = glb else {
        return (None, None);
    };
    match std::fs::read(glb)
        .map_err(anyhow::Error::from)
        .and_then(|b| Ok(Scene::from_glb(&b)?))
    {
        Ok(s) => (Some(s), None),
        Err(e) => (None, Some(format!("{e:#}"))),
    }
}

/// Сказать словами о том, что в командной строке было бы подсказкой.
fn in_words(e: nonius_app::Error) -> anyhow::Error {
    use nonius_app::Error;
    use nonius_core::project::PROJECT_FILE;
    use nonius_core::Error as Core;
    match e {
        Error::Core(Core::NotAProject(path)) => anyhow::anyhow!(
            "в папке {} нет проекта Nonius. Выберите другую папку или создайте проект",
            path.display()
        ),
        Error::Core(Core::AlreadyExists(path)) if path.ends_with(PROJECT_FILE) => {
            anyhow::anyhow!("в этой папке уже есть проект: откройте его")
        }
        Error::Core(Core::AlreadyExists(_)) => anyhow::anyhow!("такая деталь уже есть"),
        Error::Store(nonius_app::StoreError::Identity) => {
            anyhow::anyhow!("не задано имя автора версий: перезапустите Nonius, он спросит имя")
        }
        e => e.into(),
    }
}

/// То же для ошибки, уже ставшей общей.
fn plainly(e: anyhow::Error) -> anyhow::Error {
    match e.downcast() {
        Ok(e) => in_words(e),
        Err(e) => e,
    }
}

/// Компьютер для окна: git, LFS, имя автора.
struct ThisMachine;

impl Machine for ThisMachine {
    fn readiness(&self) -> Readiness {
        let r = nonius_app::readiness();
        Readiness {
            git: r.git,
            lfs: r.lfs,
            author: r.author,
        }
    }

    fn set_author(&self, name: &str) -> Result<()> {
        Ok(nonius_app::set_author(name)?)
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
                model: self.app.model_file(s.item.name.as_str()).ok().flatten(),
                stamp: self.app.stamp(&s.item),
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
                author: v.signature(),
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
        let from = version(before)?;
        if from.is_none() && before != after {
            bail!("«Было» — это сохранённая версия: сравнивать можно с ней");
        }
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
        let steps = Steps(progress);
        // Сначала деталь: она готова за секунды, цвета считаются дольше.
        let (model, model_problem) = self.model_scene(part, after, &steps);
        let Some(from) = from.filter(|_| before != after) else {
            return Ok(Comparison {
                scene: model,
                scene_problem: model_problem,
                ..Comparison::default()
            });
        };
        if let Some(scene) = &model {
            progress.model(scene.clone());
        }
        let options = CompareOptions {
            reports: true,
            render: true,
        };
        let to = version(after)?;
        let compared = self.app.compare(part, Some(from), to, &options, &steps);
        // Тела разницы освобождаются, пока OpenCascade занят этой работой.
        nonius_occt::forget_diff();
        let c = match compared {
            Ok(c) => c,
            // Цвета не получились (нет OpenCascade, булева операция не
            // сошлась) — деталь и числа всё равно покажем, а причину скажем.
            Err(nonius_app::Error::Cancelled) => bail!("сравнение отменено"),
            Err(e) => {
                let problem = match e {
                    nonius_app::Error::NoRenderer => None,
                    e => Some(format!("{e:#}")),
                };
                let numbers = CompareOptions::default();
                let c = self.app.compare(part, Some(from), to, &numbers, &steps)?;
                return Ok(Comparison {
                    step_behind: c.step_behind(),
                    scene: model,
                    scene_problem: problem,
                    reports: c.reports,
                });
            }
        };
        let (scene, scene_problem) = read_scene(&c.files);
        Ok(Comparison {
            step_behind: c.step_behind(),
            scene: scene.or(model),
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

    fn open_file(&self, part: &str, pick: Pick, path: &str, way: &OpenWay) -> Result<PathBuf> {
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

    fn programs(&self, path: &str) -> Vec<ProgramEntry> {
        nonius_app::programs_for(path)
            .into_iter()
            .map(|p| ProgramEntry {
                name: p.name,
                command: p.command,
            })
            .collect()
    }

    fn new_part(&self, name: &str, assembly: bool) -> Result<()> {
        let kind = if assembly {
            ItemKind::Assembly
        } else {
            ItemKind::Part
        };
        let _files = self.writing();
        self.app.new_item(name, kind).map_err(in_words)?;
        Ok(())
    }

    fn add_model(&self, file: &Path, part: &str, message: &str) -> Result<AddedModel> {
        // Импорт читает STEP, а OpenCascade работает по одному.
        let _one_at_a_time = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        let _files = self.writing();
        let imported = self.app.import(file, Some(part))?;
        let name = imported.item.name.to_string();
        let version = match self.app.save_version(&name, message) {
            Ok(v) => Some(v.version.get()),
            Err(nonius_app::Error::Store(nonius_app::StoreError::NoChanges(_))) => None,
            Err(e) => return Err(in_words(e)),
        };
        Ok(AddedModel {
            part: name,
            version,
            warnings: imported.warnings,
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
        opener: Arc::new(|p: &Path| AppSource::open(p).map_err(plainly)),
        creator: Arc::new(|p: &Path| AppSource::create(p).map_err(plainly)),
        machine: Arc::new(ThisMachine),
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
    fn project_and_model_from_the_window() {
        for (k, v) in [
            ("GIT_AUTHOR_NAME", "Тестировщик"),
            ("GIT_AUTHOR_EMAIL", "test@example.com"),
            ("GIT_COMMITTER_NAME", "Тестировщик"),
            ("GIT_COMMITTER_EMAIL", "test@example.com"),
        ] {
            std::env::set_var(k, v);
        }
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("Станок");
        std::fs::create_dir(&d).unwrap();
        let src = AppSource::create(&d).unwrap();
        assert_eq!(src.title(), "Станок", "название — имя папки");
        assert!(
            AppSource::create(&d).is_err(),
            "второй раз проект не создаётся"
        );

        src.new_part("корпус", true).unwrap();
        assert!(src.new_part("корпус", false).is_err(), "имя занято");
        let step = tmp.path().join("пластина.step");
        std::fs::write(&step, STEP).unwrap();
        let added = src.add_model(&step, "пластина", "Первая").unwrap();
        assert_eq!((added.part.as_str(), added.version), ("пластина", Some(1)));
        let added = src.add_model(&step, "корпус", "Модель").unwrap();
        assert_eq!((added.part.as_str(), added.version), ("корпус", Some(1)));
        let parts = src.parts().unwrap();
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|p| p.model.is_some()), "{parts:?}");
        assert!(
            parts.iter().all(|p| !p.dirty),
            "версии сохранены: {parts:?}"
        );
        assert!(parts.iter().any(|p| p.name == "корпус" && p.assembly));

        // Тот же файл ещё раз: регистр не важен, новой версии нет.
        let added = src.add_model(&step, "ПЛАСТИНА", "Снова").unwrap();
        assert_eq!((added.part.as_str(), added.version), ("пластина", None));
        assert_eq!(src.parts().unwrap().len(), 2);

        // Деталь без единой версии видна как есть: сравнивать не требуется.
        let said = Said::default();
        src.new_part("пустая", false).unwrap();
        let shown = src
            .compare("пустая", Pick::Current, Pick::Current, &said)
            .unwrap();
        assert!(shown.reports.is_empty());
    }

    #[test]
    fn broken_model_changes_nothing() {
        for (k, v) in [
            ("GIT_AUTHOR_NAME", "Тестировщик"),
            ("GIT_AUTHOR_EMAIL", "test@example.com"),
            ("GIT_COMMITTER_NAME", "Тестировщик"),
            ("GIT_COMMITTER_EMAIL", "test@example.com"),
        ] {
            std::env::set_var(k, v);
        }
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("Станок");
        std::fs::create_dir(&d).unwrap();
        let src = AppSource::create(&d).unwrap();
        let good = tmp.path().join("пластина.step");
        std::fs::write(&good, STEP).unwrap();
        src.add_model(&good, "пластина", "Первая").unwrap();

        let bad = tmp.path().join("мусор.step");
        std::fs::write(&bad, "мусор").unwrap();
        assert!(src.add_model(&bad, "новая", "Нет").is_err());
        assert!(src.add_model(&bad, "пластина", "Нет").is_err());
        let parts = src.parts().unwrap();
        assert_eq!(parts.len(), 1, "деталь-призрак: {parts:?}");
        assert!(!parts[0].dirty, "рабочая модель цела");
        let model = d.join("parts").join("пластина").join("model.step");
        assert_eq!(std::fs::read_to_string(model).unwrap(), STEP);
    }

    #[test]
    fn no_project_inside_a_project() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        nonius_app::create_project(d, Some("Станок")).unwrap();
        let inner = d.join("parts");
        let e = nonius_app::create_project(&inner, None).unwrap_err();
        assert_eq!(e.code(), "inside_project");
        assert!(e.to_string().contains("Станок"), "{e}");
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
