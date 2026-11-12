//! Долгая работа в фоне: окно не замирает, пока читаются версии и считается
//! сравнение. Каждая работа — свой поток; результат приходит в окно сообщением.
//! Сравнение можно отменить: окно сразу свободно, а поток бросает расчёт на
//! ближайшем шаге, и его результат никуда не попадает.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;

use nonius_scene::Scene;

use super::batch::Report;

use crate::source::{
    AddedModel, Comparison, DeletedEntry, DiffEntry, FileEntry, IntakeOutcome, IntakePlan, Listing,
    MassEntry, Pick, ProgramEntry, Progress, ProjectSource, VersionEntry,
};

/// Готовый результат.
pub enum Done {
    /// Проект открыт.
    Opened(anyhow::Result<Box<dyn ProjectSource>>),
    /// Список деталей.
    Parts(anyhow::Result<Listing>),
    /// Версии детали.
    Versions(String, anyhow::Result<Vec<VersionEntry>>),
    /// Список деталей при возврате в окно.
    Reread(anyhow::Result<Listing>),
    /// Детали и версии одной детали заново (после сохранения).
    Refreshed(Listing, String, anyhow::Result<Vec<VersionEntry>>),
    /// Сравнение: деталь, пара версий, результат.
    Compared(String, (Pick, Pick), anyhow::Result<Comparison>),
    /// Материал и масса: деталь, пара версий; «Было», «Стало», сейчас.
    Masses(String, (Pick, Pick), anyhow::Result<[MassEntry; 3]>),
    /// Сохранена версия детали.
    Saved(String, anyhow::Result<u32>),
    /// Материал назначен.
    MaterialSet(String, anyhow::Result<MassEntry>),
    /// Изменённые файлы детали — для окошка «Сохранить версию».
    Files(String, anyhow::Result<Vec<String>>),
    /// Файлы детали для пары версий.
    VersionFiles(String, (Pick, Pick), anyhow::Result<Vec<FileEntry>>),
    /// Строки текстового файла: деталь, пара версий, файл.
    FileDiff(String, (Pick, Pick), String, anyhow::Result<Vec<DiffEntry>>),
    /// Файл открыт или показан в папке (`true`).
    FileOpened(Pick, bool, anyhow::Result<PathBuf>),
    /// Программы для типа файла.
    Programs(String, Vec<ProgramEntry>),
    /// Создана деталь.
    PartCreated(String, anyhow::Result<()>),
    /// Создана папка: путь от корня.
    FolderCreated(String, anyhow::Result<()>),
    /// Файлы скопированы в папку-группу: пути от корня или ошибки.
    FolderFilesAdded(Vec<anyhow::Result<String>>),
    /// Настройки проекта записаны.
    SettingsSaved(anyhow::Result<()>),
    /// Файл добавлен в деталь и сохранён версией.
    Added(anyhow::Result<AddedModel>),
    /// Версии раскрытой, но не выбранной детали.
    Listed(String, anyhow::Result<Vec<VersionEntry>>),
    /// Деталь переименована: прежнее и новое имя.
    PartRenamed(String, String, anyhow::Result<()>),
    /// Деталь удалена.
    PartDeleted(String, anyhow::Result<()>),
    /// Список удалённых деталей.
    DeletedList(anyhow::Result<Vec<DeletedEntry>>),
    /// Удалённая деталь возвращена.
    PartRecovered(String, anyhow::Result<()>),
    /// Деталь возвращена к версии; результат — номер новой.
    Restored(String, u32, anyhow::Result<u32>),
    /// Файлы версии выгружены: деталь, версия, папка, сколько файлов.
    Exported(String, u32, PathBuf, anyhow::Result<usize>),
    /// Сравнение с файлом или другой деталью: деталь, подпись, результат.
    ComparedOutside(String, String, anyhow::Result<Comparison>),
    /// Свои файлы положены в деталь: пути внутри неё.
    FilesAdded(String, anyhow::Result<Vec<String>>),
    /// Файл убран из папки детали; `true` — у детали остались несохранённые
    /// изменения.
    FileRemoved(String, String, anyhow::Result<bool>),
    /// Версия убрана с карты (`true`) или возвращена: деталь, версия.
    HiddenSet(String, u32, bool, anyhow::Result<()>),
    /// Черновик перенесён: проект в новой папке (или прежний, если перенос
    /// не удался) и ошибка переноса.
    Moved(anyhow::Result<Box<dyn ProjectSource>>, Option<String>),
    /// Очередь закончилась.
    Batch(Report),
    /// Папка изучена: номер разбора и план.
    IntakeStudied(u64, anyhow::Result<IntakePlan>),
    /// План пересчитан после ответа на вопрос.
    IntakePlanned(u64, anyhow::Result<IntakePlan>),
    /// Папка разложена в проект.
    IntakeApplied(u64, anyhow::Result<IntakeOutcome>),
    /// Работа упала с внутренней ошибкой.
    Crashed,
}

/// Номер работы.
type Id = u64;

/// Очередь фоновых работ.
pub struct Jobs {
    tx: Sender<(Id, Done)>,
    rx: Receiver<(Id, Done)>,
    /// Работы, чьих результатов ждёт окно.
    running: Vec<Id>,
    next: Id,
    progress: Arc<Mutex<String>>,
    /// Деталь от идущего сравнения, пока считаются цвета.
    model: Arc<Mutex<Option<Scene>>>,
    /// Работа, которую можно отменить, и её флажок.
    cancellable: Option<(Id, Arc<AtomicBool>)>,
    /// Потоки, которые ещё не закончились, в том числе забытые и
    /// отменённые: пока они живы, они держат проект открытым.
    alive: Arc<AtomicUsize>,
}

impl Default for Jobs {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            tx,
            rx,
            running: Vec::new(),
            next: 0,
            progress: Arc::default(),
            model: Arc::default(),
            cancellable: None,
            alive: Arc::default(),
        }
    }
}

/// Ход работы → строка состояния; флажок отмены.
struct Reporter {
    ctx: egui::Context,
    progress: Arc<Mutex<String>>,
    model: Arc<Mutex<Option<Scene>>>,
    cancel: Arc<AtomicBool>,
}

impl Progress for Reporter {
    fn step(&self, what: &str) {
        // Брошенная работа не перебивает ход новой.
        if self.cancelled() {
            return;
        }
        if let Ok(mut p) = self.progress.lock() {
            *p = what.to_string();
        }
        self.ctx.request_repaint();
    }

    fn model(&self, scene: Scene) {
        if self.cancelled() {
            return;
        }
        if let Ok(mut m) = self.model.lock() {
            *m = Some(scene);
        }
        self.ctx.request_repaint();
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

impl Jobs {
    /// Запустить работу. `work` получает ход работы для сообщений.
    pub fn spawn(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce(&dyn Progress) -> Done + Send + 'static,
    ) {
        self.start(ctx, work);
    }

    /// Запустить работу, которую человек может отменить. Прошлая такая
    /// работа отменяется: её результат уже не нужен.
    pub fn spawn_cancellable(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce(&dyn Progress) -> Done + Send + 'static,
    ) {
        self.cancel();
        let started = self.start(ctx, work);
        self.cancellable = Some(started);
    }

    fn start(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce(&dyn Progress) -> Done + Send + 'static,
    ) -> (Id, Arc<AtomicBool>) {
        let id = self.next;
        self.next += 1;
        self.running.push(id);
        let cancel = Arc::new(AtomicBool::new(false));
        let reporter = Reporter {
            ctx: ctx.clone(),
            progress: self.progress.clone(),
            model: self.model.clone(),
            cancel: cancel.clone(),
        };
        let tx = self.tx.clone();
        let alive = self.alive.clone();
        alive.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            // Паника в работе не должна оставить окно в вечном ожидании.
            let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&reporter)));
            // Работа отдала всё, что держала (и проект тоже), — поток свободен.
            alive.fetch_sub(1, Ordering::SeqCst);
            let _ = tx.send((id, done.unwrap_or(Done::Crashed)));
            reporter.ctx.request_repaint();
        });
        (id, cancel)
    }

    /// Отменить работу, которую можно отменить. Окно сразу её забывает.
    /// Возвращает, было ли что отменять.
    pub fn cancel(&mut self) -> bool {
        let Some((id, flag)) = self.cancellable.take() else {
            return false;
        };
        flag.store(true, Ordering::Relaxed);
        let was = self.running.contains(&id);
        self.running.retain(|&r| r != id);
        // Шаги сообщает только отменяемая работа: её строка больше не нужна.
        self.clear_progress();
        self.take_model();
        was
    }

    /// Забрать деталь, которую прислало идущее сравнение.
    pub fn take_model(&self) -> Option<Scene> {
        self.model.lock().ok()?.take()
    }

    /// Забыть все работы: открыт другой проект, их результаты не нужны.
    pub fn forget_all(&mut self) {
        self.cancel();
        self.running.clear();
        self.clear_progress();
    }

    /// Идёт ли работа, которую можно отменить.
    pub fn can_cancel(&self) -> bool {
        self.cancellable
            .as_ref()
            .is_some_and(|(id, _)| self.running.contains(id))
    }

    /// Забрать готовые результаты. Результаты отменённых работ выбрасываются.
    pub fn finished(&mut self) -> Vec<Done> {
        let mut out = Vec::new();
        for (id, done) in self.rx.try_iter().collect::<Vec<_>>() {
            if let Some(at) = self.running.iter().position(|&r| r == id) {
                self.running.remove(at);
                out.push(done);
            }
        }
        if self.running.is_empty() {
            self.clear_progress();
        }
        out
    }

    fn clear_progress(&self) {
        if let Ok(mut p) = self.progress.lock() {
            p.clear();
        }
    }

    /// Идёт ли работа.
    pub fn busy(&self) -> bool {
        !self.running.is_empty()
    }

    /// Закончились ли все потоки, даже те, чьих результатов окно уже не ждёт.
    pub fn all_finished(&self) -> bool {
        self.alive.load(Ordering::SeqCst) == 0
    }

    /// Последнее сообщение о ходе работы.
    pub fn progress(&self) -> String {
        self.progress.lock().map(|p| p.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_arrive_and_panics_do_not_hang() {
        let ctx = egui::Context::default();
        let mut jobs = Jobs::default();
        jobs.spawn(&ctx, |p| {
            p.step("читаю");
            Done::Parts(Ok(Listing::default()))
        });
        jobs.spawn(&ctx, |_| panic!("сбой"));
        assert!(jobs.busy());
        let mut got = Vec::new();
        // Паника с трассировкой стека в отладочной сборке печатается долго.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while std::time::Instant::now() < deadline {
            got.extend(jobs.finished());
            if got.len() == 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(got.len(), 2);
        assert!(!jobs.busy());
        assert_eq!(got.iter().filter(|d| matches!(d, Done::Crashed)).count(), 1);
    }

    #[test]
    fn cancelled_work_is_forgotten_at_once() {
        let ctx = egui::Context::default();
        let mut jobs = Jobs::default();
        let (go, wait) = std::sync::mpsc::channel::<()>();
        let (seen_tx, seen) = std::sync::mpsc::channel();
        jobs.spawn_cancellable(&ctx, move |p| {
            p.step("сравниваю");
            let _ = wait.recv();
            let _ = seen_tx.send(p.cancelled());
            Done::Parts(Ok(Listing::default()))
        });
        assert!(jobs.can_cancel());
        assert!(jobs.cancel());
        // Окно свободно сразу, не дожидаясь потока.
        assert!(!jobs.busy());
        assert!(!jobs.can_cancel());
        assert!(jobs.progress().is_empty());
        go.send(()).unwrap();
        // Поток узнал об отмене.
        assert!(seen.recv().unwrap());
        // Его результат дошёл до очереди, но окну не отдаётся.
        let late = jobs
            .rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        jobs.tx.send(late).unwrap();
        assert!(jobs.finished().is_empty());
        assert!(!jobs.cancel(), "второй раз отменять нечего");
    }
}
