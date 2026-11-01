//! Окно целиком без видеокарты: список деталей, выбор, сравнение в фоне,
//! места изменений и числа в правой панели. Проект — поддельный.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use nonius_core::compare::{Change, ChangeKind, ChangeReport};
use nonius_gui::source::{
    Comparison, DiffEntry, DiffKind, FileEntry, FileState, MassEntry, MaterialEntry, OpenWay,
    PartEntry, Pick, ProgramEntry, Progress, ProjectSource, VersionEntry,
};
use nonius_gui::{App, Options, Settings};
use nonius_scene::{Mesh, Region, Role, Scene};

struct Fake {
    root: PathBuf,
    /// Что сохранили из окна: деталь и описание.
    saved: Arc<Mutex<Vec<(String, String)>>>,
    /// Что открыли из окна.
    opened: Arc<Mutex<Vec<(String, OpenWay)>>>,
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
        "Стенд".into()
    }
    fn root(&self) -> &Path {
        &self.root
    }
    fn parts(&self) -> anyhow::Result<Vec<PartEntry>> {
        Ok(vec![
            PartEntry {
                name: "кронштейн".into(),
                assembly: false,
                latest: Some("v2".into()),
                dirty: false,
                model: Some("model.step".into()),
            },
            PartEntry {
                name: "корпус".into(),
                assembly: true,
                latest: None,
                dirty: true,
                model: None,
            },
        ])
    }
    fn versions(&self, part: &str) -> anyhow::Result<Vec<VersionEntry>> {
        let v = |number| VersionEntry {
            number,
            date: "2026-10-27 12:00".into(),
            author: "Владимир".into(),
            summary: format!("версия {number}"),
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
        assert_eq!((before, after), (Pick::Version(1), Pick::Version(2)));
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
        if h.query_by_label_contains(text).is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("не дождались «{text}»");
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
            root: dir.to_path_buf(),
            saved,
            opened,
        })),
        opener: Arc::new(|_| anyhow::bail!("не нужно")),
        part: None,
        picks: None,
    };
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
    assert!(h.query_by_label_contains("История «кронштейн»").is_some());
    assert!(h.query_by_label_contains("версия 1").is_some());
    wait_for(&mut h, "126 г");
    assert!(h.query_by_label_contains("из справочника").is_some());

    // Настройки запомнились во временной папке.
    let saved = std::fs::read_to_string(dir.path().join("window.txt")).unwrap();
    assert!(saved.contains("last_project="), "{saved}");
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

    // Изменённый файл идёт первым, у него видно старый и новый размер.
    wait_for(&mut h, "~ docs/заметка.md");
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
    wait_for(&mut h, "~ docs/заметка.md");

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
