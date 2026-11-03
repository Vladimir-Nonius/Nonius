//! Сценарии человека: настоящее окно, настоящий проект на git, те же кнопки.
//! Вместо мыши — `egui_kittest`, вместо системных окон выбора — готовые ответы.
//! Форма считается только с OpenCascade, поэтому цвета проверяются в задании
//! «Геометрия», а кнопки и файлы — везде.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use nonius_app::{AiActor, App as Actions, GitOptions};
use nonius_gui::source::Chooser;
use nonius_gui::{App, Options, Settings};

use super::{plainly, AppSource, ThisMachine};
use crate::registry;

/// Настройки и переменные git общие на процесс: окна по очереди.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Сколько ждать git и OpenCascade.
const PATIENCE: Duration = Duration::from_secs(30);

/// Что человек выбрал бы в системных окнах.
#[derive(Default)]
struct Answers {
    folders: Mutex<VecDeque<PathBuf>>,
    models: Mutex<VecDeque<Vec<PathBuf>>>,
    steps: Mutex<VecDeque<PathBuf>>,
    files: Mutex<VecDeque<Vec<PathBuf>>>,
}

impl Chooser for Answers {
    fn folder(&self, _typed: &str) -> Option<PathBuf> {
        self.folders.lock().unwrap().pop_front()
    }
    fn models(&self) -> Vec<PathBuf> {
        self.models.lock().unwrap().pop_front().unwrap_or_default()
    }
    fn step(&self) -> Option<PathBuf> {
        self.steps.lock().unwrap().pop_front()
    }
    fn files(&self) -> Vec<PathBuf> {
        self.files.lock().unwrap().pop_front().unwrap_or_default()
    }
    fn picture(&self, _name: &str) -> Option<PathBuf> {
        None
    }
}

/// Рабочее место человека: папка, образцы моделей, окно.
struct Desk {
    _tmp: tempfile::TempDir,
    dir: PathBuf,
    answers: Arc<Answers>,
    h: Harness<'static, App>,
}

impl Desk {
    /// Окно без проекта на начальном экране.
    fn new() -> Self {
        for (k, v) in [
            ("GIT_AUTHOR_NAME", "Тестировщик"),
            ("GIT_AUTHOR_EMAIL", "test@example.com"),
            ("GIT_COMMITTER_NAME", "Тестировщик"),
            ("GIT_COMMITTER_EMAIL", "test@example.com"),
        ] {
            std::env::set_var(k, v);
        }
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        std::env::set_var("NONIUS_CONFIG_DIR", dir.join("настройки"));
        let answers = Arc::new(Answers::default());
        let options = Options {
            project: None,
            opener: Arc::new(|p: &Path| AppSource::open(p).map_err(plainly)),
            creator: Arc::new(|p: &Path| AppSource::create(p).map_err(plainly)),
            machine: Arc::new(ThisMachine),
            chooser: answers.clone(),
            part: None,
            picks: None,
        };
        let h = Harness::builder()
            .with_size([1280.0, 800.0])
            .build_eframe(|cc| App::new(&cc.egui_ctx, None, options, Settings::default()));
        Self {
            _tmp: tmp,
            dir,
            answers,
            h,
        }
    }

    /// Образец из `packaging/smoke`: своё имя файла, как у человека на диске.
    fn sample(&self, source: &str, name: &str) -> PathBuf {
        let smoke = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/smoke");
        let path = self.dir.join(name);
        std::fs::copy(smoke.join(source), &path).unwrap();
        path
    }

    fn wait(&mut self, text: &str) {
        let start = Instant::now();
        while start.elapsed() < PATIENCE {
            self.h.step();
            if self.sees(text) {
                self.settle();
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("не дождались «{text}», на экране: {:#?}", self.screen());
    }

    /// Все надписи окна: что было видно, когда сценарий сломался.
    fn screen(&self) -> Vec<String> {
        let all = self.h.query_all(egui_kittest::kittest::By::new());
        all.filter_map(|n| {
            let node = n.accesskit_node();
            node.label().or_else(|| node.value())
        })
        .filter(|l| !l.is_empty())
        .collect()
    }

    fn wait_gone(&mut self, text: &str) {
        let start = Instant::now();
        while start.elapsed() < PATIENCE {
            self.h.step();
            if !self.sees(text) {
                self.settle();
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("не ушло «{text}»");
    }

    /// Дать окну дорисоваться: окошко в первом кадре ещё без места на экране.
    fn settle(&mut self) {
        for _ in 0..4 {
            self.h.step();
        }
    }

    fn sees(&self, text: &str) -> bool {
        self.h.query_all_by_label_contains(text).next().is_some()
    }

    fn press(&mut self, label: &str) {
        self.h.get_by_label(label).click();
        self.settle();
    }

    /// Выбрать пункт меню окна.
    fn menu(&mut self, menu: &str, item: &str) {
        self.press(menu);
        self.press(item);
    }

    fn type_in(&mut self, text: &str) {
        {
            let mut inputs = self.h.get_all_by_role(Role::TextInput);
            let field = inputs
                .rfind(|i| i.accesskit_node().is_focused())
                .expect("курсор в поле");
            field.type_text(text);
        }
        self.settle();
    }

    /// Создать проект через начальный экран.
    fn new_project(&mut self, name: &str) -> PathBuf {
        let path = self.dir.join(name);
        std::fs::create_dir(&path).unwrap();
        self.answers.folders.lock().unwrap().push_back(path.clone());
        self.press("Создать проект…");
        self.wait("пока нет деталей");
        path
    }

    /// Положить модели, как если бы их выбрали в окне выбора.
    fn choose_models(&mut self, models: &[&Path]) {
        let list = models.iter().map(|p| p.to_path_buf()).collect();
        self.answers.models.lock().unwrap().push_back(list);
    }

    /// Бросить файл в окно.
    fn drop_file(&mut self, path: &Path) {
        self.h.input_mut().dropped_files.push(egui::DroppedFile {
            path: Some(path.to_path_buf()),
            ..Default::default()
        });
        self.settle();
    }

    /// Вернуться в окно, например из SolidWorks.
    fn come_back(&mut self) {
        self.h.input_mut().focused = false;
        self.h.step();
        self.h.input_mut().focused = true;
        self.h.step();
    }
}

/// Папка детали в проекте.
fn part_dir(project: &Path, name: &str) -> PathBuf {
    project.join("parts").join(name)
}

/// Версии в файлах: что на самом деле лежит в git.
fn saved_versions(project: &Path, part: &str) -> Vec<String> {
    let app = Actions::open(project, registry()).unwrap();
    let list = app.versions(part).unwrap();
    list.iter().map(|v| v.version.to_string()).collect()
}

#[test]
fn whole_way_of_a_part() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let mut d = Desk::new();
    assert!(d.sees("Проект не открыт"));
    let project = d.new_project("Станок");
    let one = d.sample("plate-v1.step", "плита.step");
    let two = d.sample("plate-v2.step", "плита v2.step");

    // Пустой проект сам говорит, что делать, и кнопка рядом.
    d.choose_models(&[&one]);
    d.press("Добавить модели…");
    d.wait("Будет новая деталь «плита»");
    d.press("Добавить");
    d.wait("плита   v1");
    assert_eq!(saved_versions(&project, "плита"), ["v1"]);

    // Одна версия: сравнивать не с чем, выбора «Было» и «Стало» нет.
    d.wait("Показана одна версия");
    d.wait("показана");
    assert!(!d.sees("Карта v1"), "карта одной версии не нужна");
    assert!(d.h.query_all_by_role(Role::ComboBox).count() <= 1);
    assert!(!d.sees("Удалённых"), "вкладки свёрнуты");

    // Вторая версия: имя файла подсказывает деталь.
    d.drop_file(&two);
    d.wait("Будет версия v2 «плита»");
    d.type_in("Отверстие под вал");
    d.press("Добавить");
    d.wait("Сохранена модель «плита» v2");
    d.wait("Карта v2");
    assert_eq!(saved_versions(&project, "плита"), ["v2", "v1"]);
    assert!(d.sees("Отверстие под вал"));

    // Цвета на настоящей геометрии.
    if nonius_occt::available() {
        d.wait("Без изменений");
        d.wait("Места изменений");
    }

    // Вкладки по щелчку: модель видно, служебное спрятано.
    d.h.get_by_role_and_label(Role::Button, "Файлы").click();
    d.wait("model.step");
    d.wait("Служебные файлы");
    assert!(!d.sees("meta.yaml"), "служебное свёрнуто");
    d.h.get_by_role_and_label(Role::Button, "Файлы").click();
    d.wait_gone("model.step");

    // Вернуть старую версию: новая версия с прежним содержимым.
    d.h.get_by_label("Карта v1").click();
    d.settle();
    d.menu("Деталь", "Вернуть эту версию…");
    d.wait("Вернуть «плита» к v1?");
    d.press("Вернуть");
    d.wait("возвращена к v1: v3");
    let model = part_dir(&project, "плита").join("model.step");
    assert_eq!(std::fs::read(&model).unwrap(), std::fs::read(&one).unwrap());
    assert_eq!(saved_versions(&project, "плита"), ["v3", "v2", "v1"]);

    // Переименовать: версии остаются, папка новая.
    d.menu("Деталь", "Переименовать…");
    d.type_in("плита основания");
    d.press("Переименовать");
    d.wait("теперь «плита основания»");
    d.wait("плита основания   v3");
    assert!(!part_dir(&project, "плита").exists());
    assert_eq!(saved_versions(&project, "плита основания").len(), 3);
    d.wait("Карта v2");

    // Выгрузить версию из старого имени.
    d.h.get_by_label("Карта v2").click();
    d.settle();
    let out = d.dir.join("выгрузка");
    d.answers.folders.lock().unwrap().push_back(out.clone());
    d.menu("Деталь", "Выгрузить файлы версии…");
    d.wait("выгружено файлов");
    assert_eq!(
        std::fs::read(out.join("model.step")).unwrap(),
        std::fs::read(&two).unwrap()
    );

    // Сравнить с файлом: на экране сравнение, в истории ничего нового.
    d.answers.steps.lock().unwrap().push_back(one.clone());
    d.menu("Деталь", "Сравнить с файлом…");
    d.wait("v2 → плита.step");
    d.wait_gone("Считаю изменения");
    if nonius_occt::available() {
        d.wait("Места изменений");
    }
    assert_eq!(saved_versions(&project, "плита основания").len(), 3);
    d.press("Закрыть сравнение");
    d.wait_gone("v2 → плита.step");

    // Удалить и вернуть.
    d.menu("Деталь", "Удалить…");
    d.wait("Удалить «плита основания»?");
    d.press("Удалить");
    d.wait("пока нет деталей");
    d.wait("Удалённые детали");
    assert!(!part_dir(&project, "плита основания").exists());

    d.menu("Файл", "Удалённые детали…");
    d.wait("Вернуть «плита основания»");
    d.press("Вернуть «плита основания»");
    d.wait("плита основания   v3");
    assert!(part_dir(&project, "плита основания")
        .join("model.step")
        .is_file());
    assert_eq!(saved_versions(&project, "плита основания").len(), 3);

    // Другой проект: от прошлого на экране ничего.
    let other = d.dir.join("Другой");
    std::fs::create_dir(&other).unwrap();
    d.answers.folders.lock().unwrap().push_back(other);
    d.menu("Файл", "Создать проект…");
    d.wait_gone("плита основания");
    d.wait("пока нет деталей");
}

#[test]
fn mistakes_change_nothing() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let mut d = Desk::new();
    let project = d.new_project("Станок");

    // Битый файл: причина в окошке, детали нет.
    let broken = d.dir.join("битый.step");
    std::fs::write(&broken, "это не STEP").unwrap();
    d.choose_models(&[&broken]);
    d.press("Добавить модели…");
    d.wait("Будет новая деталь «битый»");
    d.press("Добавить");
    d.wait("не STEP-файл");
    d.press("Отмена");
    d.wait("пока нет деталей");
    assert!(!part_dir(&project, "битый").exists());

    let plate = d.sample("plate-v1.step", "плита.step");
    let shaft = d.sample("plate-v1.step", "вал.step");
    d.choose_models(&[&plate, &shaft]);
    d.press("Добавить модели…");
    d.wait("Добавить все (2)");
    d.press("Добавить все (2)");
    d.wait("вал   v1");
    d.wait("плита   v1");

    // Занятое имя: окошко остаётся и говорит почему.
    d.h.get_by_label_contains("плита   v1").click();
    d.wait("Показана одна версия");
    d.menu("Деталь", "Переименовать…");
    d.type_in("ВАЛ");
    d.press("Переименовать");
    d.wait("такая деталь уже есть");
    d.press("Отмена");
    assert!(part_dir(&project, "плита").is_dir());

    // Несохранённое: переименовать и вернуть нельзя, и окно объясняет.
    let model = part_dir(&project, "плита").join("docs").join("заметка.md");
    std::fs::write(&model, "сверлить").unwrap();
    d.come_back();
    d.wait("Сейчас");
    d.menu("Деталь", "Переименовать…");
    d.wait("сначала сохраните версию");
    assert!(d.h.query_by_label("Переименовать").is_none());
    d.press("Закрыть");

    // Удалить с несохранённым: предупреждает, а файл не пропадает.
    d.menu("Деталь", "Удалить…");
    d.wait("будут сохранены версией");
    d.press("Отмена");
    assert!(part_dir(&project, "плита").is_dir());
    d.menu("Деталь", "Удалить…");
    d.wait("будут сохранены версией");
    d.press("Удалить");
    d.wait("Удалённые детали…");
    d.wait_gone("плита   v");
    d.menu("Файл", "Удалённые детали…");
    d.wait("Вернуть «плита»");
    d.press("Вернуть «плита»");
    d.wait("плита   v2");
    d.wait("Карта v2");
    assert_eq!(
        std::fs::read_to_string(&model).unwrap(),
        "сверлить",
        "несохранённое вернулось"
    );

    // Сравнить можно только со STEP: ошибка видна, показ остаётся прежним.
    let text = d.dir.join("заметка.txt");
    std::fs::write(&text, "x").unwrap();
    d.answers.steps.lock().unwrap().push_back(text);
    d.menu("Деталь", "Сравнить с файлом…");
    d.wait("сравнить можно только с файлом STEP");
    d.wait_gone("Закрыть сравнение");
}

#[test]
fn several_parts_are_open_at_once() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let mut d = Desk::new();
    let project = d.new_project("Станок");
    let plate = d.sample("plate-v1.step", "плита.step");
    let shaft = d.sample("plate-v1.step", "вал.step");
    d.choose_models(&[&plate, &shaft]);
    d.press("Добавить модели…");
    d.wait("Добавить все (2)");
    d.press("Добавить все (2)");
    d.wait("вал   v1");
    d.wait("плита   v1");

    let app = Actions::open(&project, registry()).unwrap();
    let notes = |part: &str, text: &str| {
        std::fs::write(part_dir(&project, part).join("docs").join("н.md"), text).unwrap();
        app.save_version(part, text).unwrap();
    };
    // Выбрана плита, вал раскрываем стрелкой: выбор не сбивается.
    d.h.get_by_label_contains("плита   v1").click();
    d.wait("Показана одна версия");
    d.wait("показана");
    if d.h.query_by_label("▸").is_none() {
        d.h.get_all_by_label("▾").next().unwrap().click();
        d.wait("▸");
    }
    d.press("▸");

    // Новая версия другой раскрытой детали видна сразу.
    notes("вал", "Вал вторая");
    d.come_back();
    d.wait("вал   v2");
    d.wait("Вал вторая");
    assert!(d.sees("Показана одна версия"), "выбор не сбился");

    // Версия другой детали выбирает её и показывает эту версию.
    d.h.get_by_label_contains("Вал вторая").click();
    d.wait("Карта v2");
    d.wait("стало");
}

#[test]
fn ai_works_while_the_window_is_open() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let mut d = Desk::new();
    let project = d.new_project("Станок");
    let plate = d.sample("plate-v1.step", "плита.step");
    d.choose_models(&[&plate]);
    d.press("Добавить модели…");
    d.press("Добавить");
    d.wait("Показана одна версия");

    // ИИ — другой процесс: окно его не блокирует, а изменения видит.
    let ai = AiActor::new("Claude", Some("claude-opus-5-5")).unwrap();
    let git = GitOptions {
        ai: Some(ai),
        identity: None,
    };
    let robot = Actions::open(&project, registry())
        .unwrap()
        .with_git_options(git);
    let two = d.sample("plate-v2.step", "плита2.step");
    robot.import(&two, Some("плита")).unwrap();
    robot.save_version("плита", "Правка от ИИ").unwrap();

    d.come_back();
    d.wait("Правка от ИИ");
    d.wait("Claude (ИИ, claude-opus-5-5, запустил Тестировщик)");
    d.wait("Карта v2");

    // И наоборот: деталь удалена, пока окно открыто.
    robot.delete_item("плита").unwrap();
    d.come_back();
    d.wait("пока нет деталей");
    d.wait_gone("Правка от ИИ");
}

/// Свой файл к детали: добавить из меню, убрать клавишей Delete.
#[test]
fn own_file_of_a_part() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let mut d = Desk::new();
    let project = d.new_project("Мастерская");
    let one = d.sample("plate-v1.step", "плита.step");
    d.choose_models(&[&one]);
    d.press("Добавить модели…");
    d.wait("Будет новая деталь «плита»");
    d.press("Добавить");
    d.wait("плита   v1");
    d.wait("Показана одна версия");

    let tech = d.dir.join("технология.txt");
    std::fs::write(&tech, "резать лазером").unwrap();
    d.answers.files.lock().unwrap().push_back(vec![tech]);
    d.menu("Деталь", "Добавить файл…");
    d.wait("добавлено docs/технология.txt");
    let file = part_dir(&project, "плита")
        .join("docs")
        .join("технология.txt");
    assert!(file.is_file());

    d.h.get_by_role_and_label(Role::Button, "Файлы").click();
    d.wait("+ docs/технология.txt");
    d.h.get_by_label_contains("+ docs/технология.txt").click();
    d.settle();
    d.h.key_press(egui::Key::Delete);
    d.wait("Убрать файл docs/технология.txt?");
    d.press("Убрать");
    d.wait("убран из «плита»");
    assert!(!file.exists());
    assert_eq!(
        saved_versions(&project, "плита"),
        ["v1"],
        "истории не тронута"
    );
}
