//! Окно целиком без видеокарты: список деталей, выбор, сравнение в фоне,
//! места изменений и числа в правой панели. Проект — поддельный.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use nonius_core::compare::{Change, ChangeKind, ChangeReport};
use nonius_gui::source::{Comparison, PartEntry, Pick, ProjectSource, VersionEntry};
use nonius_gui::{App, Options, Settings};
use nonius_scene::{Mesh, Region, Role, Scene};

struct Fake {
    root: PathBuf,
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
            },
            PartEntry {
                name: "корпус".into(),
                assembly: true,
                latest: None,
                dirty: true,
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
        progress: &dyn Fn(&str),
    ) -> anyhow::Result<Comparison> {
        progress("Сравниваю…");
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

#[test]
fn choose_part_and_see_places() {
    let dir = tempfile::tempdir().unwrap();
    // Настройки окна — во временную папку, не в профиль того, кто запускает тесты.
    std::env::set_var("NONIUS_CONFIG_DIR", dir.path());
    let options = Options {
        project: Some(Box::new(Fake {
            root: dir.path().to_path_buf(),
        })),
        opener: Arc::new(|_| anyhow::bail!("не нужно")),
        part: None,
        picks: None,
    };
    let mut h = Harness::builder()
        .with_size([1200.0, 700.0])
        .build_eframe(|cc| App::new(&cc.egui_ctx, None, options, Settings::default()));

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

    // Настройки запомнились во временной папке.
    let saved = std::fs::read_to_string(dir.path().join("window.txt")).unwrap();
    assert!(saved.contains("last_project="), "{saved}");
}
