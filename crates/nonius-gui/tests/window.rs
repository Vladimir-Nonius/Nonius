//! Окно целиком без видеокарты: список деталей, выбор, сравнение в фоне,
//! места изменений и числа в правой панели. Проект — поддельный.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use nonius_core::compare::{Change, ChangeKind, ChangeReport};
use nonius_gui::source::{
    AddedModel, Chooser, Comparison, DeletedEntry, DiffEntry, DiffKind, FileEntry, FileState,
    Machine, MassEntry, MaterialEntry, OpenWay, Other, PartEntry, Pick, ProgramEntry, Progress,
    ProjectSettings, ProjectSource, Readiness, VersionEntry,
};
use nonius_gui::{App, Options, Settings};
use nonius_scene::{Mesh, Region, Role, Scene};

/// Добавленная модель: файл, деталь и описание.
type Added = (PathBuf, String, String);

struct Fake {
    root: PathBuf,
    /// Что сохранили из окна: деталь и описание.
    saved: Arc<Mutex<Vec<(String, String)>>>,
    /// Что открыли из окна.
    opened: Arc<Mutex<Vec<(String, OpenWay)>>>,
    /// Какие пары версий сравнивали.
    compared: Arc<Mutex<Vec<(Pick, Pick)>>>,
    /// Какие модели добавили.
    imported: Arc<Mutex<Vec<Added>>>,
    /// Отпечаток кронштейна: меняется, когда его правят в SolidWorks.
    stamp: Arc<Mutex<String>>,
    /// Держит сравнение, пока тест смотрит на окно.
    hold: Arc<Mutex<()>>,
    /// Какие детали созданы из окна.
    created: Arc<Mutex<Vec<(String, bool)>>>,
    /// Какие версии кронштейна убраны с карты.
    hidden: Arc<Mutex<Vec<u32>>>,
    /// Какие файлы убрали из окна: деталь и путь.
    removed: Arc<Mutex<Vec<(String, String)>>>,
    /// Название проекта.
    title: &'static str,
    /// Проект старого формата; обновление его снимает.
    old: Arc<Mutex<bool>>,
    /// Папки-группы проекта.
    folders: Arc<Mutex<Vec<String>>>,
    /// Где лежат детали: деталь и папка, последнее — главное.
    moved: Arc<Mutex<Vec<(String, String)>>>,
    /// Что записали в настройки проекта.
    settings: Arc<Mutex<Vec<ProjectSettings>>>,
    /// Обычные файлы папок, путь от корня.
    loose: Arc<Mutex<Vec<String>>>,
    /// Какие свои файлы положили в детали: деталь и файл.
    attached: Arc<Mutex<Vec<(String, PathBuf)>>>,
    /// Какие детали удалили из окна.
    deleted: Arc<Mutex<Vec<String>>>,
    /// Держит удаление, пока тест смотрит на очередь.
    gate: Arc<Mutex<()>>,
}

impl Fake {
    fn folder_of(&self, part: &str) -> String {
        let moved = self.moved.lock().unwrap();
        let last = moved.iter().rev().find(|m| m.0 == part);
        last.map(|m| m.1.clone()).unwrap_or_default()
    }
}

fn tetra(at: f32) -> Mesh {
    Mesh {
        positions: vec![
            [at, 0.0, 0.0],
            [at + 10.0, 0.0, 0.0],
            [at, 10.0, 0.0],
            [at, 0.0, 10.0],
        ],
        triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    }
}

impl ProjectSource for Fake {
    fn title(&self) -> String {
        self.title.into()
    }
    fn root(&self) -> &Path {
        &self.root
    }
    fn parts(&self) -> anyhow::Result<Vec<PartEntry>> {
        let mut list = vec![
            PartEntry {
                name: "кронштейн".into(),
                assembly: false,
                latest: Some("v2".into()),
                dirty: false,
                model: Some("model.step".into()),
                stamp: self.stamp.lock().unwrap().clone(),
                folder: self.folder_of("кронштейн"),
            },
            PartEntry {
                name: "корпус".into(),
                assembly: true,
                latest: None,
                dirty: true,
                model: None,
                stamp: String::new(),
                folder: self.folder_of("корпус"),
            },
        ];
        let made = self.created.lock().unwrap();
        let added = self.imported.lock().unwrap();
        let names = made
            .iter()
            .map(|m| m.0.clone())
            .chain(added.iter().map(|a| a.1.clone()));
        for name in names {
            if !list.iter().any(|p| p.name == name) {
                list.push(PartEntry {
                    folder: self.folder_of(&name),
                    name,
                    assembly: false,
                    latest: None,
                    dirty: true,
                    model: None,
                    stamp: String::new(),
                });
            }
        }
        let deleted = self.deleted.lock().unwrap();
        list.retain(|p| !deleted.contains(&p.name));
        Ok(list)
    }
    fn settings(&self) -> ProjectSettings {
        let saved = self.settings.lock().unwrap();
        saved.last().cloned().unwrap_or(ProjectSettings {
            name: self.title.into(),
            tolerance_mm: 0.01,
        })
    }
    fn save_settings(&self, settings: &ProjectSettings) -> anyhow::Result<()> {
        if settings.name.contains('!') {
            anyhow::bail!("название «{}» не годится", settings.name);
        }
        self.settings.lock().unwrap().push(settings.clone());
        Ok(())
    }
    fn folders(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.folders.lock().unwrap().clone())
    }
    fn versions(&self, part: &str) -> anyhow::Result<Vec<VersionEntry>> {
        let v = |number| VersionEntry {
            number,
            date: "2026-10-27 12:00".into(),
            author: "Владимир".into(),
            summary: format!("версия {number}"),
            hidden: self.hidden.lock().unwrap().contains(&number),
        };
        Ok(if part == "кронштейн" {
            vec![v(2), v(1)]
        } else {
            Vec::new()
        })
    }
    fn compare(
        &self,
        _part: &str,
        before: Pick,
        after: Pick,
        progress: &dyn Progress,
    ) -> anyhow::Result<Comparison> {
        progress.step("Сравниваю…");
        progress.model(Scene {
            regions: vec![Region {
                role: Role::After,
                volume_mm3: 5000.0,
                mesh: tetra(0.0),
            }],
        });
        drop(self.hold.lock().unwrap());
        self.compared.lock().unwrap().push((before, after));
        let region = |role, at, volume_mm3| Region {
            role,
            volume_mm3,
            mesh: tetra(at),
        };
        Ok(Comparison {
            scene: Some(Scene {
                regions: vec![
                    region(Role::After, 0.0, 5000.0),
                    region(Role::Before, 0.0, 4000.0),
                    region(Role::Added, 20.0, 1680.0),
                    region(Role::Removed, 40.0, 402.0),
                ],
            }),
            scene_problem: None,
            reports: vec![ChangeReport {
                comparator: "meta-compare".into(),
                changes: vec![Change {
                    kind: ChangeKind::Modified,
                    subject: "Масса, г".into(),
                    before: Some("12.25".into()),
                    after: Some("13.4567".into()),
                }],
            }],
            step_behind: false,
        })
    }
    fn changed_files(&self, _part: &str) -> anyhow::Result<Vec<String>> {
        Ok(vec!["+ корпус.step".into()])
    }
    fn save_version(&self, part: &str, message: &str) -> anyhow::Result<u32> {
        self.saved
            .lock()
            .unwrap()
            .push((part.to_string(), message.to_string()));
        Ok(1)
    }
    fn mass(&self, part: &str, _pick: Pick) -> anyhow::Result<MassEntry> {
        Ok(if part == "кронштейн" {
            MassEntry {
                material: Some("Ст3".into()),
                density_kg_m3: Some(7850.0),
                mass_kg: Some(0.1257),
                no_mass: None,
            }
        } else {
            MassEntry {
                no_mass: Some("материал не задан".into()),
                ..MassEntry::default()
            }
        })
    }
    fn materials(&self) -> Vec<MaterialEntry> {
        vec![MaterialEntry {
            name: "Ст3".into(),
            density_kg_m3: 7850.0,
        }]
    }
    fn set_material(
        &self,
        part: &str,
        _material: Option<&str>,
        _density_kg_m3: Option<f64>,
    ) -> anyhow::Result<MassEntry> {
        self.mass(part, Pick::Current)
    }
    fn files(&self, _part: &str, _before: Pick, _after: Pick) -> anyhow::Result<Vec<FileEntry>> {
        Ok(vec![
            FileEntry {
                path: "model.step".into(),
                state: FileState::Same,
                before: Some(2048),
                after: Some(2048),
                text: false,
            },
            FileEntry {
                path: "docs/заметка.md".into(),
                state: FileState::Modified,
                before: Some(10),
                after: Some(20),
                text: true,
            },
        ])
    }
    fn file_diff(
        &self,
        _part: &str,
        _before: Pick,
        _after: Pick,
        _path: &str,
    ) -> anyhow::Result<Vec<DiffEntry>> {
        Ok(vec![DiffEntry {
            kind: DiffKind::Added,
            text: "сверлить Ø8".into(),
        }])
    }
    fn open_file(
        &self,
        _part: &str,
        _pick: Pick,
        path: &str,
        way: &OpenWay,
    ) -> anyhow::Result<PathBuf> {
        let opened = (path.to_string(), way.clone());
        self.opened.lock().unwrap().push(opened);
        Ok(self.root.join(path))
    }
    fn programs(&self, _path: &str) -> Vec<ProgramEntry> {
        vec![program()]
    }
    fn new_part(&self, name: &str, assembly: bool, folder: &str) -> anyhow::Result<()> {
        if name == "кронштейн" {
            anyhow::bail!("такая деталь уже есть");
        }
        self.created
            .lock()
            .unwrap()
            .push((name.to_string(), assembly));
        if !folder.is_empty() {
            self.move_part(name, folder)?;
        }
        Ok(())
    }
    fn new_folder(&self, folder: &str) -> anyhow::Result<()> {
        let mut folders = self.folders.lock().unwrap();
        if folders.iter().any(|f| f == folder) {
            anyhow::bail!("папка {folder} уже есть");
        }
        folders.push(folder.to_string());
        Ok(())
    }
    fn move_part(&self, part: &str, folder: &str) -> anyhow::Result<()> {
        let moved = (part.to_string(), folder.to_string());
        self.moved.lock().unwrap().push(moved);
        Ok(())
    }
    fn add_model(&self, file: &Path, part: &str, message: &str) -> anyhow::Result<AddedModel> {
        if file.to_string_lossy().contains("битый") {
            anyhow::bail!("файл не читается");
        }
        let entry = (file.to_path_buf(), part.to_string(), message.to_string());
        self.imported.lock().unwrap().push(entry);
        Ok(AddedModel {
            part: part.to_string(),
            version: Some(1),
            warnings: vec!["превью не сделано".into()],
        })
    }
    fn rename_part(&self, _part: &str, new: &str) -> anyhow::Result<()> {
        anyhow::ensure!(new != "корпус", "такая деталь уже есть");
        Ok(())
    }
    fn delete_part(&self, part: &str) -> anyhow::Result<()> {
        let _wait = self.gate.lock().unwrap();
        self.deleted.lock().unwrap().push(part.to_string());
        Ok(())
    }
    fn deleted_parts(&self) -> anyhow::Result<Vec<DeletedEntry>> {
        Ok(Vec::new())
    }
    fn recover_part(&self, _part: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn restore_version(&self, _part: &str, _version: u32) -> anyhow::Result<u32> {
        Ok(3)
    }
    fn add_file(&self, part: &str, file: &Path) -> anyhow::Result<String> {
        let added = (part.to_string(), file.to_path_buf());
        self.attached.lock().unwrap().push(added);
        Ok(format!("docs/{}", file.display()))
    }
    fn folder_files(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.loose.lock().unwrap().clone())
    }
    fn add_folder_file(&self, folder: &str, file: &Path) -> anyhow::Result<String> {
        let name = file.file_name().unwrap().to_string_lossy();
        let path = if folder.is_empty() {
            name.to_string()
        } else {
            format!("{folder}/{name}")
        };
        self.loose.lock().unwrap().push(path.clone());
        Ok(path)
    }
    fn remove_folder_file(&self, path: &str) -> anyhow::Result<()> {
        self.loose.lock().unwrap().retain(|f| f != path);
        Ok(())
    }
    fn open_folder_file(&self, path: &str, way: &OpenWay) -> anyhow::Result<PathBuf> {
        let opened = (path.to_string(), way.clone());
        self.opened.lock().unwrap().push(opened);
        Ok(self.root.join(path))
    }
    fn remove_file(&self, part: &str, path: &str) -> anyhow::Result<()> {
        self.removed
            .lock()
            .unwrap()
            .push((part.to_string(), path.to_string()));
        Ok(())
    }
    fn set_hidden(&self, _part: &str, version: u32, hidden: bool) -> anyhow::Result<()> {
        let mut list = self.hidden.lock().unwrap();
        list.retain(|n| *n != version);
        if hidden {
            list.push(version);
        }
        Ok(())
    }
    fn export_version(&self, _part: &str, _version: u32, _dest: &Path) -> anyhow::Result<usize> {
        Ok(2)
    }
    fn old_layout(&self) -> bool {
        *self.old.lock().unwrap()
    }

    fn upgrade(&self) -> anyhow::Result<()> {
        *self.old.lock().unwrap() = false;
        Ok(())
    }

    fn compare_outside(
        &self,
        part: &str,
        pick: Pick,
        _other: &Other,
        progress: &dyn Progress,
    ) -> anyhow::Result<Comparison> {
        self.compare(part, pick, Pick::Current, progress)
    }
}

/// Окна выбора в тестах: человека нет, отвечать нечем.
struct NoDialogs;

impl Chooser for NoDialogs {
    fn folder(&self, _typed: &str) -> Option<PathBuf> {
        None
    }
    fn models(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn files(&self) -> Vec<PathBuf> {
        vec![PathBuf::from("технология.txt")]
    }
    fn step(&self) -> Option<PathBuf> {
        None
    }
    fn picture(&self, _name: &str) -> Option<PathBuf> {
        None
    }
}

/// Компьютер, на котором всё есть, или без имени автора.
#[derive(Default)]
struct Pc {
    no_author: bool,
    authors: Arc<Mutex<Vec<String>>>,
}

impl Machine for Pc {
    fn readiness(&self) -> Readiness {
        Readiness {
            git: true,
            lfs: true,
            author: !self.no_author && self.authors.lock().unwrap().is_empty(),
        }
    }
    fn set_author(&self, name: &str) -> anyhow::Result<()> {
        self.authors.lock().unwrap().push(name.to_string());
        Ok(())
    }
}

fn program() -> ProgramEntry {
    ProgramEntry {
        name: "Компас".into(),
        command: "kompas.exe \"%1\"".into(),
    }
}

/// Шагать окно, пока не появится надпись (работа идёт в фоне).
fn wait_for(h: &mut Harness<'_, App>, text: &str) {
    for _ in 0..300 {
        h.step();
        if h.query_all_by_label_contains(text).next().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let all = h.query_all(egui_kittest::kittest::By::new());
    let seen: Vec<_> = all
        .filter_map(|n| {
            let node = n.accesskit_node();
            node.label().or_else(|| node.value())
        })
        .filter(|l| !l.is_empty())
        .collect();
    panic!("не дождались «{text}», на экране: {seen:#?}");
}

/// Окно с поддельным проектом; настройки — во временной папке, не в
/// профиле того, кто запускает тесты.
/// Папку настроек задаёт переменная окружения — окна по очереди.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn window(dir: &Path, saved: Arc<Mutex<Vec<(String, String)>>>) -> Harness<'static, App> {
    window_with(dir, saved, Arc::default())
}

fn window_with(
    dir: &Path,
    saved: Arc<Mutex<Vec<(String, String)>>>,
    opened: Arc<Mutex<Vec<(String, OpenWay)>>>,
) -> Harness<'static, App> {
    std::env::set_var("NONIUS_CONFIG_DIR", dir);
    let options = Options {
        project: Some(Box::new(Fake {
            saved,
            opened,
            ..fake(dir, "Стенд")
        })),
        opener: Arc::new(|_| anyhow::bail!("не нужно")),
        creator: Arc::new(|_| anyhow::bail!("не нужно")),
        machine: Arc::new(Pc::default()),
        chooser: Arc::new(NoDialogs),
        part: None,
        picks: None,
        drafts: None,
        mover: Arc::new(|_, _| anyhow::bail!("не нужно")),
        discard: Arc::new(|d| Ok(std::fs::remove_dir_all(d)?)),
    };
    build(options)
}

fn fake(dir: &Path, title: &'static str) -> Fake {
    Fake {
        root: dir.to_path_buf(),
        saved: Arc::default(),
        opened: Arc::default(),
        compared: Arc::default(),
        imported: Arc::default(),
        stamp: Arc::default(),
        hold: Arc::default(),
        created: Arc::default(),
        removed: Arc::default(),
        hidden: Arc::default(),
        title,
        old: Arc::default(),
        folders: Arc::default(),
        moved: Arc::default(),
        settings: Arc::default(),
        loose: Arc::default(),
        attached: Arc::default(),
        deleted: Arc::default(),
        gate: Arc::default(),
    }
}

fn build(options: Options) -> Harness<'static, App> {
    Harness::builder()
        .with_size([1200.0, 800.0])
        .build_eframe(|cc| App::new(&cc.egui_ctx, None, options, Settings::default()))
}

#[test]
fn choose_part_and_see_places() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut h = window(dir.path(), Arc::default());

    wait_for(&mut h, "кронштейн");
    assert!(h.query_by_label_contains("корпус").is_some());
    // Без видеокарты окно честно говорит, что 3D нет, а остальное работает.
    assert!(h.query_by_label_contains("видеокарта").is_some());

    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "Добавлено 1");
    assert!(h.query_by_label_contains("Убрано 1").is_some());
    // Объём — и в легенде, и в списке мест.
    assert_eq!(h.query_all_by_label_contains("1\u{a0}680 мм³").count(), 2);
    assert!(h
        .query_by_label_contains("Масса, г: 12,25 → 13,46")
        .is_some());
    // История: обе версии с описанием; материал и масса.
    assert!(
        h.query_by_label_contains("Сейчас").is_none(),
        "изменений нет"
    );
    assert!(h.query_by_label_contains("версия 1").is_some());
    wait_for(&mut h, "126 г");
    assert!(h.query_by_label_contains("из справочника").is_some());

    // Настройки запомнились во временной папке.
    let saved = std::fs::read_to_string(dir.path().join("window.txt")).unwrap();
    assert!(saved.contains("tab="), "{saved}");
}

#[test]
fn save_first_version_from_the_window() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let saved = Arc::new(Mutex::new(Vec::new()));
    let mut h = window(dir.path(), saved.clone());
    wait_for(&mut h, "корпус");
    h.get_by_label_contains("корпус").click();
    wait_for(&mut h, "Сохранить первую версию");
    // Без материала масса не считается — окно так и говорит.
    wait_for(&mut h, "материал не задан");
    h.get_by_label_contains("Сохранить первую версию").click();
    wait_for(&mut h, "корпус.step");
    // Без описания сохранить нельзя.
    h.get_by_label_contains("Сохранить как v1").click();
    h.run();
    assert!(saved.lock().unwrap().is_empty());
    h.get_by_role(egui::accesskit::Role::MultilineTextInput)
        .click();
    h.run();
    h.get_by_role(egui::accesskit::Role::MultilineTextInput)
        .type_text("Первая версия корпуса");
    h.run();
    h.get_by_label_contains("Сохранить как v1").click();
    wait_for(&mut h, "Сохранена версия «корпус» v1");
    assert_eq!(
        *saved.lock().unwrap(),
        [("корпус".to_string(), "Первая версия корпуса".to_string())]
    );
}

#[test]
fn files_and_numbers_tabs() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let opened = Arc::new(Mutex::new(Vec::new()));
    let mut h = window_with(dir.path(), Arc::default(), opened.clone());
    wait_for(&mut h, "кронштейн");
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "Добавлено 1");

    // Вкладки свёрнуты, пока их не попросят: 3D важнее.
    assert!(h.query_by_label_contains("docs/заметка.md").is_none());
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Файлы")
        .click();

    // Изменённый файл идёт первым, у него видно старый и новый размер.
    wait_for(&mut h, "~ docs/заметка.md");
    for _ in 0..4 {
        h.step();
    }
    assert!(h.query_by_label_contains("10 байт → 20 байт").is_some());
    assert!(h.query_by_label_contains("model.step   2,0 КБ").is_some());

    h.get_by_label_contains("docs/заметка.md").click();
    wait_for(&mut h, "сверлить Ø8");
    assert!(h.query_by_label_contains("изменён").is_some());

    h.get_by_label("Открыть v2").click();
    wait_for(&mut h, "Открыт файл");
    assert!(h.query_by_label_contains("копия").is_some());
    assert_eq!(
        *opened.lock().unwrap(),
        [("docs/заметка.md".to_string(), OpenWay::Default)]
    );

    h.get_by_role_and_label(egui::accesskit::Role::Button, "Числа")
        .click();
    wait_for(&mut h, "Параметр");
    assert!(h.query_by_label_contains("Было (v1)").is_some());
    assert!(
        h.query_by_label("+1,21").is_some(),
        "разница массы в граммах"
    );
    assert!(
        h.query_all_by_label("Ст3").count() >= 2,
        "материал в обеих колонках"
    );

    // Щелчок по открытой вкладке сворачивает панель.
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Числа")
        .click();
    h.run();
    assert!(h.query_by_label_contains("Параметр").is_none());
}

#[test]
fn right_click_menu() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let opened = Arc::new(Mutex::new(Vec::new()));
    let mut h = window_with(dir.path(), Arc::default(), opened.clone());
    wait_for(&mut h, "кронштейн");
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "Добавлено 1");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Файлы")
        .click();
    wait_for(&mut h, "~ docs/заметка.md");
    for _ in 0..4 {
        h.step();
    }

    // Файл версии: меню предупреждает про копию и показывает программы.
    h.get_by_label_contains("~ docs/заметка.md")
        .click_secondary();
    wait_for(&mut h, "Копия только для чтения");
    h.get_by_label_contains("Открыть в…").click_accesskit();
    wait_for(&mut h, "Компас");
    h.get_by_label("Компас").click();
    wait_for(&mut h, "Открыт файл");

    h.get_by_label_contains("~ docs/заметка.md")
        .click_secondary();
    wait_for(&mut h, "Показать в папке");
    h.get_by_label("Показать в папке").click();
    wait_for(&mut h, "Показан в папке");

    // Деталь: всегда «Сейчас», поэтому без предупреждения, файл — модель.
    h.get_by_label_contains("кронштейн   v2").click_secondary();
    wait_for(&mut h, "Открыть в…");
    assert!(h.query_by_label_contains("Копия только").is_none());
    h.get_by_label("Показать в папке").click();
    for _ in 0..100 {
        h.step();
        if opened.lock().unwrap().len() == 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let path = |p: &str| p.to_string();
    assert_eq!(
        *opened.lock().unwrap(),
        [
            (path("docs/заметка.md"), OpenWay::Program(program())),
            (path("docs/заметка.md"), OpenWay::Folder),
            (path("model.step"), OpenWay::Folder),
        ]
    );
}

/// Окно, где видно, что попросили у проекта.
fn window_on(dir: &Path, project: Fake) -> Harness<'static, App> {
    std::env::set_var("NONIUS_CONFIG_DIR", dir);
    build(Options {
        project: Some(Box::new(project)),
        opener: Arc::new(|p| Ok(Box::new(self::fake(p, "Второй")))),
        creator: Arc::new(|_| anyhow::bail!("не нужно")),
        machine: Arc::new(Pc::default()),
        chooser: Arc::new(NoDialogs),
        part: None,
        picks: None,
        drafts: None,
        mover: Arc::new(|_, _| anyhow::bail!("не нужно")),
        discard: Arc::new(|d| Ok(std::fs::remove_dir_all(d)?)),
    })
}

fn drop_file(h: &mut Harness<'_, App>, path: &Path) {
    h.input_mut().dropped_files.push(egui::DroppedFile {
        path: Some(path.to_path_buf()),
        ..Default::default()
    });
}

#[test]
fn project_buttons_are_always_there() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut h = window(dir.path(), Arc::default());
    wait_for(&mut h, "кронштейн");
    assert!(h.query_by_label("Открыть проект…").is_none(), "свёрнуто");
    h.get_by_label("Файл").click();
    h.run();
    for label in [
        "Открыть проект…",
        "Новый проект",
        "Новая деталь…",
        "Добавить модели…",
        "Добавить папку с моделями…",
    ] {
        assert!(h.query_by_label(label).is_some(), "нет кнопки {label}");
    }
}

#[test]
fn start_screen_opens_and_creates() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("NONIUS_CONFIG_DIR", dir.path());
    let mut h = build(Options {
        project: None,
        opener: Arc::new(|_| anyhow::bail!("не нужно")),
        creator: Arc::new(|p| Ok(Box::new(self::fake(p, "Новый")))),
        machine: Arc::new(Pc::default()),
        chooser: Arc::new(NoDialogs),
        part: None,
        picks: None,
        drafts: None,
        mover: Arc::new(|_, _| anyhow::bail!("не нужно")),
        discard: Arc::new(|d| Ok(std::fs::remove_dir_all(d)?)),
    });
    h.run();
    assert!(h.query_by_label("Открыть проект…").is_some());
    assert!(h.query_by_label("Новый проект").is_some());

    // Модели в пустое окно: проект создаётся рядом с ними и сразу принимает их.
    let models = dir.path().join("модели");
    std::fs::create_dir(&models).unwrap();
    std::fs::write(models.join("плита.step"), "x").unwrap();
    drop_file(&mut h, &models);
    wait_for(&mut h, "Создать проект здесь?");
    h.run();
    h.get_by_label("Создать").click();
    wait_for(&mut h, "Будет новая деталь «плита»");
    assert!(h.query_all_by_label("Новый").next().is_some());
}

#[test]
fn another_project_opens_in_its_own_tab() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let second = dir.path().join("второй");
    std::fs::create_dir(&second).unwrap();
    std::fs::write(second.join("project.yaml"), "").unwrap();
    let mut h = window_on(dir.path(), fake(dir.path(), "Стенд"));
    wait_for(&mut h, "кронштейн");
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "Добавлено 1");

    drop_file(&mut h, &second);
    wait_for(&mut h, "Второй");
    assert!(
        h.query_all_by_label("Стенд").next().is_some(),
        "первая вкладка"
    );
    assert!(
        h.query_by_label_contains("Добавлено 1").is_none(),
        "сравнение первого проекта"
    );
    assert!(
        h.query_by_label_contains("версия 1").is_none(),
        "история первого проекта"
    );

    // Тот же проект второй раз не открывается: окно переходит к нему.
    drop_file(&mut h, &second);
    h.run();
    assert_eq!(h.query_all_by_label("×").count(), 2);

    // Назад к первому: его деталь и сравнение снова на месте.
    h.query_all_by_label("Стенд").next().unwrap().click();
    wait_for(&mut h, "Добавлено 1");
    let saved = Settings::load();
    assert_eq!(saved.tabs, vec![dir.path().to_path_buf(), second.clone()]);
    assert_eq!(saved.active, 0);

    // Закрыть первую: остаётся вторая.
    h.query_all_by_label("×").next().unwrap().click();
    h.run();
    assert!(h.query_by_label("Стенд").is_none());
    assert_eq!(Settings::load().tabs, vec![second]);
}

#[test]
fn new_part_from_the_window() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let created = project.created.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");

    h.get_by_label("Файл").click();
    wait_for(&mut h, "Новая деталь…");
    h.get_by_label("Новая деталь…").click();
    wait_for(&mut h, "Это сборка");
    // Поле названия уже в фокусе: щелчок мимо окошка закрыл бы его.
    h.run();
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap()
        .type_text("кронштейн");
    h.run();
    h.get_by_label("Создать").click();
    wait_for(&mut h, "такая деталь уже есть");

    // Окошко осталось: имя можно исправить.
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap()
        .type_text("2");
    h.run();
    h.get_by_label("Это сборка").click();
    h.run();
    h.get_by_label("Создать").click();
    wait_for(&mut h, "Создана деталь «кронштейн2»");
    assert_eq!(*created.lock().unwrap(), [("кронштейн2".to_string(), true)]);
    assert!(h.query_by_label("кронштейн2").is_some());
}

#[test]
fn dropped_model_asks_and_saves_a_version() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let imported = project.imported.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");

    // Новая деталь: первая версия подписана сама, остаётся нажать «Добавить».
    let plate = dir.path().join("плита #2.STEP");
    drop_file(&mut h, &plate);
    wait_for(&mut h, "Будет новая деталь «плита _2»");
    h.run();
    h.get_by_label("Добавить").click();
    wait_for(&mut h, "Сохранена модель «плита _2» v1");
    assert!(h.query_by_label_contains("превью не сделано").is_some());
    assert_eq!(
        *imported.lock().unwrap(),
        [(plate, "плита _2".to_string(), "Первая версия".to_string())]
    );

    // То же имя, только другими буквами: новая версия, и нужно описание.
    drop_file(&mut h, &dir.path().join("КРОНШТЕЙН.step"));
    wait_for(&mut h, "Будет версия v3 «кронштейн»");
    h.run();
    assert!(
        h.get_by_label("Добавить").accesskit_node().is_disabled(),
        "без описания нельзя"
    );
    let inputs = h.get_all_by_role(egui::accesskit::Role::TextInput);
    inputs.last().unwrap().type_text("Сверлим Ø8");
    h.run();
    h.get_by_label("Добавить").click();
    wait_for(&mut h, "Сохранена модель «кронштейн» v1");
    assert_eq!(imported.lock().unwrap()[1].2, "Сверлим Ø8");
}

#[test]
fn target_is_chosen_from_the_list() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let imported = project.imported.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");

    drop_file(&mut h, &dir.path().join("совсем другое имя.step"));
    wait_for(&mut h, "Будет новая деталь «совсем другое имя»");
    h.run();
    // Выбор в списке не закрывает окошко.
    h.get_by_role(egui::accesskit::Role::ComboBox).click();
    h.run();
    h.get_all_by_label("кронштейн").last().unwrap().click();
    h.run();
    wait_for(&mut h, "Будет версия v3 «кронштейн»");
    let inputs = h.get_all_by_role(egui::accesskit::Role::TextInput);
    inputs.last().unwrap().focus();
    h.run();
    let inputs = h.get_all_by_role(egui::accesskit::Role::TextInput);
    inputs.last().unwrap().type_text("Другая форма");
    h.run();
    h.get_by_label("Добавить").click();
    wait_for(&mut h, "Сохранена модель «кронштейн» v1");
    assert_eq!(imported.lock().unwrap()[0].1, "кронштейн");
}

#[test]
fn many_models_and_a_broken_one() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("узел");
    std::fs::create_dir(&folder).unwrap();
    for file in ["а.step", "б.step", "битый.step", "заметка.txt"] {
        std::fs::write(folder.join(file), "x").unwrap();
    }
    let project = fake(dir.path(), "Стенд");
    let imported = project.imported.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");

    drop_file(&mut h, &folder);
    wait_for(&mut h, "Добавить модель 1 из 3");
    h.run();
    h.get_by_label_contains("Добавить все (3)").click();
    // Битый файл останавливает очередь и ничего не меняет.
    wait_for(&mut h, "файл не читается");
    assert_eq!(imported.lock().unwrap().len(), 2);
    h.get_by_label("Отмена").click();
    wait_for(&mut h, "Добавлено моделей: 2");
    assert!(h.query_by_label_contains("а   ").is_none());
    assert!(h.query_by_label_contains("битый").is_none(), "нет призрака");
}

#[test]
fn errors_are_seen_and_can_be_closed() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut h = window(dir.path(), Arc::default());
    wait_for(&mut h, "кронштейн");
    drop_file(&mut h, &dir.path().join("заметка.txt"));
    wait_for(&mut h, "Здесь нет моделей");
    h.get_by_label("✕").click();
    h.run();
    assert!(h.query_by_label_contains("Здесь нет моделей").is_none());
}

#[test]
fn old_colors_go_away_while_counting() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let hold = project.hold.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "Добавлено 1");

    let held = hold.lock().unwrap();
    h.get_by_label("Карта v1").click();
    wait_for(&mut h, "Считаю изменения…");
    assert!(h.query_by_label_contains("Добавлено 1").is_none());
    assert!(h.query_by_label_contains("Масса, г").is_none());
    drop(held);
    wait_for(&mut h, "Показана одна версия");
}

#[test]
fn changes_from_outside_are_shown_on_return() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let compared = project.compared.clone();
    let stamp = project.stamp.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "Добавлено 1");
    let count = compared.lock().unwrap().len();

    let leave_and_return = |h: &mut Harness<'_, App>| {
        h.input_mut().focused = false;
        h.step();
        h.input_mut().focused = true;
        h.step();
    };
    // Ничего не менялось: выбор и сравнение остаются.
    leave_and_return(&mut h);
    for _ in 0..30 {
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(compared.lock().unwrap().len(), count);

    *stamp.lock().unwrap() = "после SolidWorks".into();
    leave_and_return(&mut h);
    for _ in 0..200 {
        h.step();
        if compared.lock().unwrap().len() > count {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(compared.lock().unwrap().len() > count, "сравнить заново");
}

#[test]
fn first_start_asks_the_name() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("NONIUS_CONFIG_DIR", dir.path());
    let authors: Arc<Mutex<Vec<String>>> = Arc::default();
    let mut h = build(Options {
        project: None,
        opener: Arc::new(|_| anyhow::bail!("не нужно")),
        creator: Arc::new(|_| anyhow::bail!("не нужно")),
        machine: Arc::new(Pc {
            no_author: true,
            authors: authors.clone(),
        }),
        chooser: Arc::new(NoDialogs),
        part: None,
        picks: None,
        drafts: None,
        mover: Arc::new(|_, _| anyhow::bail!("не нужно")),
        discard: Arc::new(|d| Ok(std::fs::remove_dir_all(d)?)),
    });
    wait_for(&mut h, "Как вас подписывать?");
    h.run();
    // На других системах за окошком есть ещё поле пути к проекту.
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.is_focused())
        .unwrap()
        .type_text("Владимир");
    h.run();
    h.get_by_label("Запомнить").click();
    h.run();
    assert_eq!(*authors.lock().unwrap(), ["Владимир"]);
    assert!(h.query_by_label("Как вас подписывать?").is_none());
}

#[test]
fn tree_search_and_collapse() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut h = window(dir.path(), Arc::default());
    wait_for(&mut h, "кронштейн");
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "версия 1");

    // Стрелка сворачивает и разворачивает версии выбранной детали.
    h.get_by_label("▾").click();
    h.run();
    assert!(h.query_by_label_contains("версия 1").is_none());
    h.get_by_label_contains("кронштейн   v2").click();
    h.run();
    assert!(h.query_by_label_contains("версия 1").is_some());

    h.get_by_role(egui::accesskit::Role::TextInput).click();
    h.run();
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("КОРП");
    h.run();
    assert!(h.query_by_label_contains("кронштейн   v2").is_none());
    assert!(h.query_by_label_contains("корпус").is_some());
}

#[test]
fn map_shows_versions() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let compared = project.compared.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");
    assert!(
        h.query_by_label("Карта версий").is_none(),
        "деталь не выбрана"
    );
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "Карта v2");
    wait_for(&mut h, "Добавлено 1");

    // Щелчок: версия становится «Стало», «Было» — предыдущей, её нет.
    h.get_by_label("Карта v1").click();
    for _ in 0..100 {
        h.step();
        if compared.lock().unwrap().len() == 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        compared.lock().unwrap().last(),
        Some(&(Pick::Version(1), Pick::Version(1)))
    );
    // Правая кнопка: то же меню, что в истории.
    h.get_by_label("Карта v2").click_secondary();
    wait_for(&mut h, "Сделать «Было»");
    assert!(h.query_by_label("Сделать «Стало»").is_some());
}

#[test]
fn right_click_acts_on_its_own_row() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut h = window(dir.path(), Arc::default());
    wait_for(&mut h, "кронштейн");
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "Добавлено 1");

    // Выбран кронштейн, а щёлкнули по корпусу: удалять и называть — корпус.
    h.get_by_label_contains("корпус").click_secondary();
    wait_for(&mut h, "Удалить…");
    h.get_by_label("Удалить…").click();
    wait_for(&mut h, "«корпус»?");
    assert!(h.query_by_label_contains("«кронштейн»?").is_none());
    h.get_by_label("Отмена").click();
    for _ in 0..4 {
        h.step();
    }

    // Кронштейн больше не выбран, но раскрыт: у его версии своё меню.
    h.get_by_label("v1").click_secondary();
    wait_for(&mut h, "Вернуть эту версию");
    assert!(h.query_by_label_contains("Сделать «Было»").is_none());
    h.get_by_label_contains("Вернуть эту версию").click();
    wait_for(&mut h, "«кронштейн» к v1?");
}

#[test]
fn version_model_opens_from_its_menu() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let opened = project.opened.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "версия 1");

    // Правая кнопка по старой версии: открыть её модель копией.
    h.get_by_label("v1").click_secondary();
    wait_for(&mut h, "Открыть в…");
    assert!(h.query_by_label("Показать в папке").is_some());
    h.get_by_label("Открыть").click();
    wait_for(&mut h, "(копия, правки в ней не попадут в историю)");
    assert_eq!(
        *opened.lock().unwrap(),
        [("model.step".to_string(), OpenWay::Default)]
    );
}

#[test]
fn own_files_from_the_files_tab() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let removed = project.removed.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "Добавлено 1");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Файлы")
        .click();
    wait_for(&mut h, "~ docs/заметка.md");
    for _ in 0..4 {
        h.step();
    }

    // Модель так не убирают: пункта нет.
    h.get_by_label_contains("model.step").click_secondary();
    wait_for(&mut h, "Показать в папке");
    assert!(h.query_by_label("Удалить файл…").is_none());
    h.get_by_label("Показать в папке").click();
    h.run();

    // Свой файл: меню, вопрос, убран.
    h.get_by_label_contains("~ docs/заметка.md")
        .click_secondary();
    wait_for(&mut h, "Удалить файл…");
    // Меню встаёт на место со второго кадра.
    h.run();
    h.get_by_label("Удалить файл…").click();
    wait_for(&mut h, "Удалить файл docs/заметка.md?");
    h.run();
    h.get_by_label("Удалить").click();
    wait_for(&mut h, "убран из «кронштейн»");
    let gone = ("кронштейн".to_string(), "docs/заметка.md".to_string());
    assert_eq!(*removed.lock().unwrap(), [gone]);

    // Delete по выбранному файлу спрашивает так же. Сначала окно перечитает
    // деталь после «убран».
    for _ in 0..50 {
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    wait_for(&mut h, "~ docs/заметка.md");
    h.get_by_label_contains("~ docs/заметка.md").click();
    h.run();
    h.key_press(egui::Key::Delete);
    wait_for(&mut h, "Удалить файл docs/заметка.md?");
    h.run();
    h.get_by_label("Отмена").click();
    h.run();
    assert_eq!(removed.lock().unwrap().len(), 1);

    h.get_by_label("Добавить файл…").click();
    wait_for(&mut h, "добавлено docs/технология.txt");
}

#[test]
fn version_leaves_the_map_and_comes_back() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut h = window(dir.path(), Arc::default());
    wait_for(&mut h, "кронштейн");
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "Карта v1");

    // Старая версия уходит с карты, но остаётся в истории.
    h.get_by_label("Карта v1").click_secondary();
    wait_for(&mut h, "Убрать с карты");
    h.run();
    h.get_by_label("Убрать с карты").click();
    wait_for(&mut h, "v1 «кронштейн» убрана с карты");
    for _ in 0..30 {
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    // Одна версия на карте — карта не нужна, а в истории строка «Убрано».
    assert!(h.query_by_label("Карта v1").is_none());
    assert!(h.query_by_label("Карта v2").is_none());
    h.get_by_label_contains("Убрано с карты (1)").click();
    wait_for(&mut h, "версия 1");
    h.get_by_label("v1").click_secondary();
    wait_for(&mut h, "Вернуть на карту");
    h.run();
    h.get_by_label("Вернуть на карту").click();
    wait_for(&mut h, "v1 «кронштейн» снова на карте");
    wait_for(&mut h, "Карта v1");
}

/// Окна выбора: папка всегда эта.
struct FolderIs(PathBuf);

impl Chooser for FolderIs {
    fn folder(&self, _typed: &str) -> Option<PathBuf> {
        Some(self.0.clone())
    }
    fn models(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn files(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn step(&self) -> Option<PathBuf> {
        None
    }
    fn picture(&self, _name: &str) -> Option<PathBuf> {
        None
    }
}

#[test]
fn draft_without_a_folder_is_saved_later() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("NONIUS_CONFIG_DIR", dir.path());
    let drafts = dir.path().join("drafts");
    let target = dir.path().join("Мастерская");
    let mut h = build(Options {
        project: None,
        opener: Arc::new(|p| Ok(Box::new(self::fake(p, "Мастерская")))),
        creator: Arc::new(|p| {
            std::fs::create_dir_all(p)?;
            Ok(Box::new(self::fake(p, "Без имени")))
        }),
        machine: Arc::new(Pc::default()),
        chooser: Arc::new(FolderIs(target.clone())),
        part: None,
        picks: None,
        drafts: Some(drafts.clone()),
        mover: Arc::new(|from, to| Ok(std::fs::rename(from, to)?)),
        discard: Arc::new(|d| Ok(std::fs::remove_dir_all(d)?)),
    });

    // Сразу черновик: работать можно, папку никто не спрашивал.
    wait_for(&mut h, "Черновик: проект пока во временной папке");
    wait_for(&mut h, "кронштейн");
    assert!(h.query_by_label("Без имени ●").is_some());
    let draft = std::fs::read_dir(&drafts)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();

    // Крестик у черновика с деталями — сначала вопрос; «Отмена» оставляет всё.
    h.get_by_label("×").click();
    wait_for(&mut h, "Проект не сохранён");
    h.run();
    h.get_by_label("Отмена").click();
    h.run();
    assert!(h.query_by_label("Проект не сохранён").is_none());
    assert!(h.query_by_label("Без имени ●").is_some());

    // «Сохранить проект как…»: черновик переезжает в выбранную папку.
    h.get_by_label("Сохранить проект как…").click();
    wait_for(&mut h, "Мастерская");
    h.run();
    assert!(h.query_by_label_contains("Черновик").is_none());
    assert!(target.is_dir());
    assert!(!draft.exists(), "папка черновика убрана");
    assert_eq!(Settings::load().tabs, vec![target.clone()]);

    // Новый черновик рядом и «Не сохранять»: вкладка и папка пропадают.
    h.get_by_label("+").click();
    h.run();
    h.get_by_label("Новый проект").click();
    wait_for(&mut h, "Без имени ●");
    wait_for(&mut h, "Черновик");
    h.query_all_by_label("×").nth(1).unwrap().click();
    wait_for(&mut h, "Проект не сохранён");
    h.run();
    h.get_by_label("Не сохранять").click();
    h.run();
    assert!(h.query_by_label("Без имени ●").is_none());
    assert_eq!(std::fs::read_dir(&drafts).unwrap().count(), 0);
    assert_eq!(Settings::load().tabs, vec![target]);
}

#[test]
fn old_project_is_upgraded_from_the_window() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let old = Arc::new(Mutex::new(true));
    let mut h = window_on(
        dir.path(),
        Fake {
            old: old.clone(),
            ..fake(dir.path(), "Стенд")
        },
    );
    wait_for(&mut h, "Проект старого формата");
    h.get_by_label("Не сейчас").click();
    h.run();
    assert!(h.query_by_label("Не сейчас").is_none(), "спросили один раз");
    assert!(
        h.query_by_label_contains("только просмотр").is_some(),
        "полоса остаётся"
    );
    assert!(*old.lock().unwrap(), "без согласия не обновляется");

    h.get_by_label("Обновить").click();
    wait_for(&mut h, "Второй");
    assert!(!*old.lock().unwrap(), "обновлён");
    assert!(h.query_by_label_contains("только просмотр").is_none());
}

/// Протащить мышью от одной строки к другой и отпустить.
fn drag(h: &mut Harness<'_, App>, from: egui::Rect, to: egui::Rect) {
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    let start = from.center();
    let steps = [start, start + egui::vec2(0.0, 12.0), to.center()];
    h.input_mut().events.push(egui::Event::PointerMoved(start));
    h.input_mut().events.push(press(start, true));
    h.run_steps(3);
    for pos in steps {
        h.input_mut().events.push(egui::Event::PointerMoved(pos));
        h.run_steps(3);
    }
    h.input_mut().events.push(press(to.center(), false));
    h.run_steps(3);
}

#[test]
fn folders_in_the_tree() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    project.folders.lock().unwrap().push("Стена".into());
    project.move_part("кронштейн", "Стена").unwrap();
    let (moved, created) = (project.moved.clone(), project.created.clone());
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");
    assert!(h.query_by_label_contains("Стена").is_some());

    // Свернуть папку — деталь в ней не видна, раскрыть — снова видна.
    h.get_by_label_contains("Стена").click();
    h.run();
    assert!(h.query_by_label_contains("кронштейн").is_none());
    h.get_by_label_contains("Стена").click();
    h.run();
    assert!(h.query_by_label_contains("кронштейн").is_some());

    // Папка выбрана щелчком: новая папка ляжет внутрь неё.
    h.get_by_label("+ папка").click();
    wait_for(&mut h, "В папке «Стена»");
    h.run();
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap()
        .type_text("Рейка");
    h.run();
    h.get_by_label("Создать").click();
    wait_for(&mut h, "Создана папка «Стена/Рейка»");

    // Новая деталь — в только что созданную папку.
    h.get_by_label("+ деталь").click();
    wait_for(&mut h, "В папке «Стена/Рейка»");
    h.run();
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap()
        .type_text("крюк");
    h.run();
    h.get_by_label("Создать").click();
    wait_for(&mut h, "Создана деталь «крюк»");
    assert_eq!(created.lock().unwrap()[0].0, "крюк");
    assert!(moved
        .lock()
        .unwrap()
        .contains(&("крюк".to_string(), "Стена/Рейка".to_string())));
}

#[test]
fn drag_part_into_folder() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    project.folders.lock().unwrap().push("Стена".into());
    let moved = project.moved.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "корпус");

    let part = h.get_by_label_contains("корпус").rect();
    let folder = h.get_by_label_contains("Стена").rect();
    drag(&mut h, part, folder);
    wait_for(&mut h, "«корпус» перенесена в «Стена»");
    assert_eq!(
        *moved.lock().unwrap(),
        [("корпус".to_string(), "Стена".to_string())]
    );
    // Щелчок без сдвига мыши — по-прежнему выбор, а не перенос.
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "2026-10-27");
    assert_eq!(moved.lock().unwrap().len(), 1);
}

/// Поле ввода с этим текстом.
fn field<'a>(h: &'a Harness<'_, App>, value: &str) -> egui_kittest::Node<'a> {
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.value().as_deref() == Some(value))
        .unwrap_or_else(|| panic!("нет поля «{value}»"))
}

/// Дописать в конец поля.
fn append(h: &mut Harness<'_, App>, value: &str, text: &str) {
    field(h, value).click();
    h.run_steps(3);
    h.key_press(egui::Key::End);
    field(h, value).type_text(text);
    h.run_steps(3);
}

#[test]
fn structure_shows_folders_and_blanks() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    project.folders.lock().unwrap().push("Стена".into());
    let moved = project.moved.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "корпус");

    h.get_by_label("Структура").click();
    wait_for(&mut h, "Корень проекта");
    // У корпуса модели нет: это заготовка, модель добавляется прямо здесь.
    assert!(h.query_by_label("сборка · заготовка").is_some());
    assert!(h.query_by_label("1 деталь · 1 сборка").is_some());
    assert!(h.query_by_label("Добавить модель…").is_some());
    assert!(h.query_all_by_label_contains("Пусто").next().is_some());

    let card = h.get_by_label("корпус").rect();
    let folder = h.get_all_by_label("🗀 Стена").next().unwrap().rect();
    drag(&mut h, card, folder);
    wait_for(&mut h, "«корпус» перенесена в «Стена»");
    assert_eq!(
        *moved.lock().unwrap(),
        [("корпус".to_string(), "Стена".to_string())]
    );

    // Двойной щелчок по карточке — деталь в 3D.
    // Оба щелчка в одном кадре: между кадрами проверка ждёт дольше,
    // чем длится двойной щелчок.
    let at = h.get_by_label("кронштейн").rect().center();
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    for pressed in [true, false, true, false] {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
    }
    wait_for(&mut h, "2026-10-27");
    assert!(
        h.query_by_label("Корень проекта").is_none(),
        "показана деталь"
    );
}

#[test]
fn project_settings_from_the_window() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let saved = project.settings.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");

    h.get_by_label("Файл").click();
    wait_for(&mut h, "Настройки…");
    h.get_by_label("Настройки…").click();
    wait_for(&mut h, "Допуск сравнения, мм");
    // Окошко ещё выплывает: щелчок по нему раньше времени попал бы мимо.
    h.run_steps(10);

    h.get_by_label("Программа").click();
    wait_for(&mut h, "Колесо мыши наоборот");
    h.get_by_label("Проект").click();
    wait_for(&mut h, "Допуск сравнения, мм");

    // Не вышло — окно остаётся и говорит почему.
    append(&mut h, "Стенд", "!");
    h.get_by_label("Сохранить").click();
    wait_for(&mut h, "не годится");
    // Щелчок по «Сохранить» увёл фокус: сначала снова в поле.
    field(&h, "Стенд!").click();
    h.run_steps(3);
    h.key_press(egui::Key::End);
    h.key_press(egui::Key::Backspace);
    h.run_steps(3);

    append(&mut h, "0.01", "5");
    h.get_by_label("Сохранить").click();
    // Проект открывается заново: его заменит «Второй» из открывателя проверки.
    wait_for(&mut h, "Второй");
    let last = saved.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        last,
        ProjectSettings {
            name: "Стенд".into(),
            tolerance_mm: 0.015
        }
    );
}

/// Бросить файлы из Проводника, когда курсор над этим местом.
fn drop_at(h: &mut Harness<'_, App>, at: egui::Pos2, files: &[PathBuf]) {
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.run_steps(2);
    for f in files {
        h.input_mut().dropped_files.push(egui::DroppedFile {
            path: Some(f.clone()),
            ..Default::default()
        });
    }
    h.run_steps(2);
}

#[test]
fn files_from_explorer_go_where_dropped() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    project.folders.lock().unwrap().push("Стена".into());
    let (loose, attached, opened) = (
        project.loose.clone(),
        project.attached.clone(),
        project.opened.clone(),
    );
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "корпус");
    let pdf = dir.path().join("ТЗ стены.pdf");
    std::fs::write(&pdf, "чертёж").unwrap();

    // На папку — копия прямо в неё, и она видна в дереве.
    let folder = h.get_by_label_contains("Стена").rect().center();
    drop_at(&mut h, folder, std::slice::from_ref(&pdf));
    wait_for(&mut h, "🗋 ТЗ стены.pdf");
    assert_eq!(*loose.lock().unwrap(), ["Стена/ТЗ стены.pdf"]);

    // Двойной щелчок по файлу открывает его.
    let at = h
        .get_all_by_label("🗋 ТЗ стены.pdf")
        .next()
        .unwrap()
        .rect()
        .center();
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    for pressed in [true, false, true, false] {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
    }
    wait_for(&mut h, "Открыт");
    assert_eq!(opened.lock().unwrap()[0].0, "Стена/ТЗ стены.pdf");

    // На деталь — в её docs/.
    let part = h.get_by_label_contains("корпус").rect().center();
    drop_at(&mut h, part, std::slice::from_ref(&pdf));
    wait_for(&mut h, "«корпус»: добавлено");
    assert_eq!(*attached.lock().unwrap(), [("корпус".to_string(), pdf)]);
}

#[test]
fn dropped_file_goes_where_it_was_carried() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    project.folders.lock().unwrap().push("Стена".into());
    let loose = project.loose.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "корпус");
    let pdf = dir.path().join("лист.pdf");
    std::fs::write(&pdf, "чертёж").unwrap();

    // Файл несут над папкой: у курсора написано, куда он ляжет.
    let folder = h.get_by_label_contains("Стена").rect().center();
    h.input_mut().events.push(egui::Event::PointerMoved(folder));
    h.input_mut().hovered_files.push(egui::HoveredFile {
        path: Some(pdf.clone()),
        ..Default::default()
    });
    h.step();
    h.step();
    assert!(h.query_by_label("Скопировать в «Стена»").is_some());

    // Кадр броска пришёл, когда мышь уже ушла, — файл всё равно в «Стена».
    let away = h.get_by_label_contains("корпус").rect().center();
    h.input_mut().events.push(egui::Event::PointerMoved(away));
    h.input_mut().dropped_files.push(egui::DroppedFile {
        path: Some(pdf),
        ..Default::default()
    });
    h.step();
    wait_for(&mut h, "🗋 лист.pdf");
    assert_eq!(*loose.lock().unwrap(), ["Стена/лист.pdf"]);
}

#[test]
fn folder_files_have_the_same_menu_and_delete() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    project.folders.lock().unwrap().push("Стена".into());
    let loose = project.loose.clone();
    loose.lock().unwrap().push("Стена/лист.dxf".into());
    loose.lock().unwrap().push("Стена/ТЗ.pdf".into());
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "🗋 лист.dxf");

    // Правый щелчок — то же меню, что у файлов детали.
    h.get_all_by_label("🗋 лист.dxf")
        .next()
        .unwrap()
        .click_secondary();
    wait_for(&mut h, "Открыть в…");
    assert!(h.query_by_label("Показать в папке").is_some());
    h.get_by_label("Удалить файл…").click();
    wait_for(&mut h, "Удалить файл Стена/лист.dxf?");
    // Окошко ещё выплывает: щелчок раньше времени попал бы мимо.
    h.run_steps(10);
    h.get_by_label("Удалить").click();
    wait_for(&mut h, "файл удалён");
    assert_eq!(*loose.lock().unwrap(), ["Стена/ТЗ.pdf"]);

    // Delete по выделенному файлу — тот же вопрос.
    h.get_all_by_label("🗋 ТЗ.pdf").next().unwrap().click();
    h.run_steps(2);
    h.key_press(egui::Key::Delete);
    wait_for(&mut h, "Удалить файл Стена/ТЗ.pdf?");
}

#[test]
fn file_dropped_anywhere_in_a_structure_folder_lands_there() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    project.folders.lock().unwrap().push("Стена".into());
    let loose = project.loose.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "корпус");
    h.get_by_label("Структура").click();
    wait_for(&mut h, "Корень проекта");
    let pdf = dir.path().join("лист.pdf");
    std::fs::write(&pdf, "чертёж").unwrap();

    // Не на заголовок, а на «Пусто…» внутри блока папки.
    let inside = h
        .get_all_by_label_contains("Пусто")
        .next()
        .unwrap()
        .rect()
        .center();
    drop_at(&mut h, inside, std::slice::from_ref(&pdf));
    wait_for(&mut h, "Скопировано");
    assert_eq!(*loose.lock().unwrap(), ["Стена/лист.pdf"]);
}

#[test]
fn model_dropped_on_a_folder_becomes_a_part_there() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    project.folders.lock().unwrap().push("Стена".into());
    let (moved, loose, imported) = (
        project.moved.clone(),
        project.loose.clone(),
        project.imported.clone(),
    );
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "корпус");
    let shaft = dir.path().join("вал.STEP");
    let pdf = dir.path().join("лист.pdf");
    std::fs::write(&shaft, "x").unwrap();
    std::fs::write(&pdf, "чертёж").unwrap();

    // Модель — новой деталью прямо в папку, чертёж — обычным файлом рядом.
    let folder = h.get_by_label_contains("Стена").rect().center();
    drop_at(&mut h, folder, &[shaft.clone(), pdf]);
    wait_for(&mut h, "Будет новая деталь «вал» в папке «Стена»");
    h.run();
    h.get_by_label("Добавить").click();
    wait_for(&mut h, "Сохранена модель «вал» v1");
    assert_eq!(imported.lock().unwrap()[0].1, "вал");
    assert!(moved
        .lock()
        .unwrap()
        .contains(&("вал".to_string(), "Стена".to_string())));
    assert_eq!(*loose.lock().unwrap(), ["Стена/лист.pdf"]);

    // Такое имя уже есть в другом месте: окошко говорит где.
    let bracket = dir.path().join("кронштейн.step");
    std::fs::write(&bracket, "x").unwrap();
    let folder = h
        .get_all_by_label_contains("Стена")
        .find(|n| n.accesskit_node().label().is_some_and(|l| l.contains("🗀")))
        .unwrap()
        .rect()
        .center();
    drop_at(&mut h, folder, std::slice::from_ref(&bracket));
    wait_for(&mut h, "Будет версия v3 «кронштейн»");
    assert!(h
        .query_by_label_contains("Деталь с таким именем уже лежит в корне проекта")
        .is_some());
}

#[test]
fn part_dropped_anywhere_in_a_folder_moves_there() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    project.folders.lock().unwrap().push("Стена".into());
    let moved = project.moved.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "корпус");
    h.get_by_label("Структура").click();
    wait_for(&mut h, "Корень проекта");

    // Карточку — за строку версии, не за имя; не на заголовок, а на
    // «Пусто…» внутри блока папки.
    let card = h.get_by_label("сборка · заготовка").rect();
    let inside = h.get_all_by_label_contains("Пусто").next().unwrap().rect();
    drag(&mut h, card, inside);
    wait_for(&mut h, "«корпус» перенесена в «Стена»");

    // В дереве — на деталь, что уже лежит в папке: в ту же папку.
    h.get_by_label("Деталь (3D)").click();
    h.run_steps(3);
    let part = h.get_by_label_contains("кронштейн").rect();
    // Строка дерева, а не надпись «… перенесена» снизу.
    let there = h
        .get_all_by_label_contains("корпус")
        .find(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with("корпус"))
        })
        .unwrap()
        .rect();
    drag(&mut h, part, there);
    wait_for(&mut h, "«кронштейн» перенесена в «Стена»");
    assert_eq!(
        *moved.lock().unwrap(),
        [
            ("корпус".to_string(), "Стена".to_string()),
            ("кронштейн".to_string(), "Стена".to_string())
        ]
    );
}

#[test]
fn delete_key_asks_about_the_chosen_part() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut h = window_on(dir.path(), fake(dir.path(), "Стенд"));
    wait_for(&mut h, "корпус");
    h.get_by_label_contains("корпус").click();
    h.run_steps(3);
    h.key_press(egui::Key::Delete);
    wait_for(&mut h, "Удалить «корпус»?");
}

#[test]
fn structure_card_has_the_part_menu() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut h = window_on(dir.path(), fake(dir.path(), "Стенд"));
    wait_for(&mut h, "корпус");
    h.get_by_label("Структура").click();
    wait_for(&mut h, "Корень проекта");
    // Правая кнопка по любому месту карточки, а не только по имени.
    h.get_by_label("сборка · заготовка").click_secondary();
    wait_for(&mut h, "Переименовать…");
    h.run_steps(5);
    h.get_by_label("Удалить…").click();
    wait_for(&mut h, "Удалить «корпус»?");
}

/// Щелчок с Ctrl или Shift. egui берёт их из общего состояния клавиш, а не
/// из события щелчка: зажать на время щелчка и отпустить.
fn click_with(h: &mut Harness<'_, App>, at: egui::Rect, modifiers: egui::Modifiers) {
    let pos = at.center();
    h.input_mut().modifiers = modifiers;
    h.input_mut().events.push(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        });
    }
    h.run_steps(2);
    h.input_mut().modifiers = egui::Modifiers::default();
    h.run_steps(2);
}

/// Строка дерева, что начинается с `name`. Она говорит, выделена ли;
/// имя на карточке «Структуры» — нет.
fn row(h: &Harness<'_, App>, name: &str) -> egui::Rect {
    h.query_all_by_label_contains(name)
        .find(|n| {
            let node = n.accesskit_node();
            node.label().is_some_and(|l| l.starts_with(name)) && node.toggled().is_some()
        })
        .unwrap_or_else(|| panic!("нет строки «{name}»"))
        .rect()
}

/// Подсвечена ли строка дерева, что начинается с `name`.
fn chosen(h: &Harness<'_, App>, name: &str) -> bool {
    h.query_all_by_label_contains(name).any(|n| {
        let node = n.accesskit_node();
        let on = node.toggled() == Some(egui::accesskit::Toggled::True);
        node.label().is_some_and(|l| l.starts_with(name)) && on
    })
}

#[test]
fn ctrl_click_adds_without_opening() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let compared = project.compared.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "Добавлено 1");
    let count = compared.lock().unwrap().len();

    let body = row(&h, "корпус");
    click_with(&mut h, body, egui::Modifiers::COMMAND);
    assert!(chosen(&h, "кронштейн") && chosen(&h, "корпус"));
    // Открытым остался кронштейн: сравнение то же.
    h.run_steps(10);
    assert_eq!(compared.lock().unwrap().len(), count);
    assert!(h.query_by_label_contains("Добавлено 1").is_some());

    // Ctrl ещё раз — снять.
    click_with(&mut h, body, egui::Modifiers::COMMAND);
    assert!(chosen(&h, "кронштейн") && !chosen(&h, "корпус"));
}

#[test]
fn shift_in_structure_selects_a_run_seen_in_the_tree() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    project.created.lock().unwrap().push(("щит".into(), true));
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "щит");
    h.get_by_label("Структура").click();
    wait_for(&mut h, "Корень проекта");

    h.get_by_label("кронштейн").click();
    h.run_steps(3);
    // Имя «щит» и у строки дерева, и у карточки: карточка — та, что без
    // выделения.
    let card = |h: &Harness<'_, App>| {
        h.get_all_by_label("щит")
            .find(|n| n.accesskit_node().toggled().is_none())
            .unwrap()
            .rect()
    };
    let last = card(&h);
    click_with(&mut h, last, egui::Modifiers::SHIFT);
    for name in ["кронштейн", "корпус", "щит"] {
        assert!(chosen(&h, name), "в дереве выделен «{name}»");
    }

    // Пустое место «Структуры» снимает выделение.
    let below = card(&h).translate(egui::vec2(0.0, 300.0));
    click_with(&mut h, below, egui::Modifiers::default());
    assert!(!chosen(&h, "корпус") && !chosen(&h, "кронштейн"));
}

#[test]
fn vanished_part_leaves_the_selection() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let created = project.created.clone();
    created.lock().unwrap().push(("щит".into(), true));
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "щит");
    h.get_by_label_contains("корпус").click();
    h.run_steps(3);
    let shield = row(&h, "щит");
    click_with(&mut h, shield, egui::Modifiers::COMMAND);
    assert!(chosen(&h, "корпус") && chosen(&h, "щит"));

    // «Щит» удалили снаружи; окно перечитывает проект по возвращении.
    created.lock().unwrap().clear();
    h.input_mut().focused = false;
    h.step();
    h.input_mut().focused = true;
    for _ in 0..200 {
        h.step();
        if h.query_all_by_label_contains("щит").next().is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(h.query_all_by_label_contains("щит").next().is_none());
    // Выделен остался один корпус: Delete — про него.
    h.key_press(egui::Key::Delete);
    wait_for(&mut h, "Удалить «корпус»?");
}

#[test]
fn delete_two_with_one_question() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let deleted = project.deleted.clone();
    let loose = project.loose.clone();
    loose.lock().unwrap().push("ТЗ.pdf".into());
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "ТЗ.pdf");
    h.get_by_label_contains("корпус").click();
    h.run_steps(3);
    let file = row(&h, "🗋 ТЗ.pdf");
    click_with(&mut h, file, egui::Modifiers::COMMAND);
    h.key_press(egui::Key::Delete);
    // Одно окошко на всё: детали и файлы по отдельности.
    wait_for(&mut h, "Удалить выделенное (2)?");
    // Окошко встаёт на место со второго кадра.
    h.run();
    assert!(h.query_by_label_contains("«корпус»").is_some());
    assert!(h.query_by_label_contains("«ТЗ.pdf»").is_some());
    h.get_by_label("Удалить").click();
    wait_for(&mut h, "Удалено 2 из 2");
    assert_eq!(*deleted.lock().unwrap(), ["корпус"]);
    assert!(loose.lock().unwrap().is_empty());
    for _ in 0..100 {
        h.step();
        if h.query_all_by_label_contains("корпус").next().is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(h.query_all_by_label_contains("корпус").next().is_none());
}

#[test]
fn queue_stops_after_the_current_one() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let deleted = project.deleted.clone();
    let gate = project.gate.clone();
    project.created.lock().unwrap().push(("щит".into(), true));
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "щит");
    // Правая кнопка по одному из выделенных — меню про все три.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    for name in ["кронштейн", "корпус", "щит"] {
        assert!(chosen(&h, name), "выделен «{name}»");
    }
    let held = gate.lock().unwrap();
    h.get_by_label_contains("корпус").click_secondary();
    wait_for(&mut h, "Удалить выделенное (3)…");
    h.run();
    h.get_by_label("Удалить выделенное (3)…").click();
    wait_for(&mut h, "Удалить выделенное (3)?");
    h.run();
    h.get_by_label("Удалить").click();
    wait_for(&mut h, "Удаляю 1 из 3");
    h.get_by_label("Остановить").click();
    h.run_steps(2);
    drop(held);
    wait_for(&mut h, "Удалено 1 из 3, остановлено");
    assert_eq!(deleted.lock().unwrap().len(), 1);
}

#[test]
fn keys_for_the_chosen() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let compared = project.compared.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");
    // Ctrl+A — всё в дереве, Esc — ничего.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    assert!(chosen(&h, "кронштейн") && chosen(&h, "корпус"));
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(!chosen(&h, "кронштейн") && !chosen(&h, "корпус"));
    // Ctrl выделяет, не открывая; Enter открывает.
    let body = row(&h, "кронштейн");
    click_with(&mut h, body, egui::Modifiers::COMMAND);
    assert!(compared.lock().unwrap().is_empty());
    h.key_press(egui::Key::Enter);
    wait_for(&mut h, "Добавлено 1");
    // F2 — переименовать.
    h.key_press(egui::Key::F2);
    wait_for(&mut h, "Переименовать «кронштейн»");
}

#[test]
fn clicking_the_open_part_keeps_its_comparison() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let compared = project.compared.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "кронштейн");
    h.get_by_label_contains("кронштейн").click();
    wait_for(&mut h, "Карта v1");
    h.get_by_label("Карта v1").click();
    for _ in 0..100 {
        h.step();
        if compared.lock().unwrap().last() == Some(&(Pick::Version(1), Pick::Version(1))) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let count = compared.lock().unwrap().len();

    // Пустое место под деревом снимает выделение, открытая остаётся.
    let below = row(&h, "корпус").translate(egui::vec2(0.0, 60.0));
    click_with(&mut h, below, egui::Modifiers::default());
    assert!(!chosen(&h, "кронштейн"));
    // Щелчок по ней снова выделяет, но не открывает заново.
    let body = row(&h, "кронштейн");
    click_with(&mut h, body, egui::Modifiers::default());
    assert!(chosen(&h, "кронштейн"));
    for _ in 0..30 {
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(compared.lock().unwrap().len(), count);
    assert_eq!(
        compared.lock().unwrap().last(),
        Some(&(Pick::Version(1), Pick::Version(1)))
    );
}

#[test]
fn mouse_frame_selects_without_keys() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = fake(dir.path(), "Стенд");
    let compared = project.compared.clone();
    let mut h = window_on(dir.path(), project);
    wait_for(&mut h, "корпус");

    // Дерево: справа от строк пусто — рамка оттуда задевает строки по высоте.
    let right = |r: egui::Rect| {
        egui::Rect::from_center_size(egui::pos2(200.0, r.center().y), egui::vec2(2.0, 2.0))
    };
    let from = right(row(&h, "кронштейн"));
    let to = right(row(&h, "корпус"));
    drag(&mut h, from, to);
    assert!(chosen(&h, "кронштейн") && chosen(&h, "корпус"));
    // Рамка только выделяет: в 3D ничего не открылось.
    assert!(compared.lock().unwrap().is_empty());

    // «Структура»: рамка с пустого места под карточками накрывает одну.
    h.get_by_label("Структура").click();
    wait_for(&mut h, "Корень проекта");
    let card = h.get_by_label("кронштейн").rect();
    let below = egui::Rect::from_center_size(
        egui::pos2(card.left() + 4.0, card.bottom() + 150.0),
        egui::vec2(2.0, 2.0),
    );
    drag(&mut h, below, card);
    assert!(chosen(&h, "кронштейн") && !chosen(&h, "корпус"));
}
