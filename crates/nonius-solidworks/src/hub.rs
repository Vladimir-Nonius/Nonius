//! Один сеанс со своим SolidWorks на всю программу: очередь к нему и его
//! состояние наружу.
//!
//! Кто бы ни просил STEP или связи файлов — импорт, разбор папки, «Показать
//! в 3D», — все идут сюда и к одному и тому же SolidWorks: параллельно его
//! не запускают. Очередь одна, с двумя ступенями: сначала то, чего ждёт
//! человек ([`Ask::urgent`]), потом фоновое (уточнение разбора папки).
//! Команду, которая уже идёт, срочная просьба не прерывает — встаёт сразу
//! за ней.
//!
//! Ждут очереди здесь, без чужих замков: мост окна свою очередь к
//! OpenCascade на время ожидания SolidWorks должен отпускать сам.
//!
//! Состояние ([`Status`]) — для индикатора в окне: читать [`status`] можно
//! из любой нити и сколько угодно часто, SolidWorks от этого не будится.
//! Простоял без дела — свой SolidWorks закрывается сам, вежливо;
//! понадобится снова — поднимется.

use std::sync::{Condvar, Mutex, MutexGuard, Once, OnceLock};
use std::time::Duration;

use crate::session::{Failed, Hooks, Limits, Phase, Session};
use crate::wire::{Command, Own};
use crate::worker::PowerShell;

/// После стольких секунд без дела свой SolidWorks закрывается: он занимает
/// сотни мегабайт памяти.
const IDLE: Duration = Duration::from_secs(60);

/// Что сейчас со своим SolidWorks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// SolidWorks на этом компьютере не установлен.
    Missing,
    /// Свободен: не запущен или ждёт команды.
    Idle,
    /// Работает над файлом.
    Working {
        /// Имя файла (без папки).
        file: String,
        /// Какой это файл по счёту…
        done: usize,
        /// …из скольких: в пачке того, кто просит, или в очереди.
        total: usize,
    },
    /// Запускается или перезапускается.
    Restarting,
}

/// Пожелания того, кто просит.
pub(crate) struct Ask<'a> {
    /// Ждёт человек: вперёд фоновых просьб.
    pub urgent: bool,
    /// Место в пачке просящего: «3 из 27».
    pub place: Option<(usize, usize)>,
    /// Строка хода для человека.
    pub notify: &'a (dyn Fn(&str) + Sync),
    /// Попросили ли остановить.
    pub cancelled: &'a (dyn Fn() -> bool + Sync),
}

impl Ask<'static> {
    /// Срочно, молча и без отмены.
    pub fn plain() -> Self {
        fn silent(_: &str) {}
        fn never() -> bool {
            false
        }
        Self {
            urgent: true,
            place: None,
            notify: &silent,
            cancelled: &never,
        }
    }
}

/// Доска очереди: кто ждёт и что делается.
#[derive(Default)]
struct Board {
    busy: bool,
    /// Сколько ждёт срочных и фоновых просьб.
    urgent: usize,
    background: usize,
    phase: Option<Phase>,
    file: String,
    place: Option<(usize, usize)>,
    /// Сколько команд выполнено с тех пор, как очередь была пуста.
    served: usize,
    /// Свой SolidWorks, если он запущен.
    own: Option<Own>,
}

/// Общий сеанс.
pub(crate) struct Hub {
    board: Mutex<Board>,
    turn: Condvar,
    session: Mutex<Session>,
}

static HUB: OnceLock<Hub> = OnceLock::new();

/// Сеанс этой программы.
pub(crate) fn hub() -> &'static Hub {
    static KEEPER: Once = Once::new();
    let hub = HUB.get_or_init(|| {
        Hub::new(Session::new(
            Box::new(PowerShell::default()),
            Limits::default(),
        ))
    });
    // Нить, которая закрывает простаивающий SolidWorks. Когда программа
    // завершается, она просто исчезает: SolidWorks закроет рабочий процесс.
    KEEPER.call_once(|| {
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(5));
            hub.close_idle(IDLE);
        });
    });
    hub
}

/// Что сейчас со своим SolidWorks. Ничего не запускает.
pub fn status() -> Status {
    if !crate::run::available() {
        return Status::Missing;
    }
    HUB.get().map_or(Status::Idle, Hub::status)
}

/// Свой экземпляр SolidWorks, если он сейчас запущен: номер процесса и
/// время старта. По ним видно, что это не SolidWorks человека.
pub fn own() -> Option<Own> {
    HUB.get().and_then(|hub| hub.board().own.clone())
}

impl Hub {
    pub fn new(session: Session) -> Self {
        Self {
            board: Mutex::default(),
            turn: Condvar::new(),
            session: Mutex::new(session),
        }
    }

    fn board(&self) -> MutexGuard<'_, Board> {
        self.board.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Состояние сеанса.
    pub fn status(&self) -> Status {
        let board = self.board();
        match board.phase {
            None => Status::Idle,
            Some(Phase::Restarting) => Status::Restarting,
            Some(Phase::Working) => {
                let (done, total) = board.place.unwrap_or((
                    board.served + 1,
                    board.served + 1 + board.urgent + board.background,
                ));
                Status::Working {
                    file: board.file.clone(),
                    done,
                    total,
                }
            }
        }
    }

    /// Выполнить команду в своём SolidWorks, дождавшись очереди.
    pub fn run(&self, command: &Command, ask: &Ask) -> Result<Vec<String>, Failed> {
        self.wait_turn(command, ask)?;
        let phase = |phase| self.board().phase = Some(phase);
        let hooks = Hooks {
            notify: ask.notify,
            cancelled: ask.cancelled,
            phase: &phase,
        };
        let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        let result = session.run(command, &hooks);
        let own = session.own().cloned();
        drop(session);
        let mut board = self.board();
        board.own = own;
        board.busy = false;
        board.phase = None;
        board.served += 1;
        if board.urgent + board.background == 0 {
            board.served = 0;
        }
        drop(board);
        self.turn.notify_all();
        result
    }

    /// Встать в очередь и дождаться своей: срочные — вперёд фоновых.
    fn wait_turn(&self, command: &Command, ask: &Ask) -> Result<(), Failed> {
        let mut board = self.board();
        let count = |board: &mut Board, by: isize| {
            let line = if ask.urgent {
                &mut board.urgent
            } else {
                &mut board.background
            };
            *line = line.saturating_add_signed(by);
        };
        count(&mut board, 1);
        while board.busy || (!ask.urgent && board.urgent > 0) {
            if (ask.cancelled)() {
                count(&mut board, -1);
                return Err(Failed::Cancelled);
            }
            let waited = self.turn.wait_timeout(board, Duration::from_millis(100));
            board = waited.unwrap_or_else(|e| e.into_inner()).0;
        }
        count(&mut board, -1);
        board.busy = true;
        board.phase = Some(Phase::Working);
        board.place = ask.place;
        let name = command.subject().and_then(|file| file.file_name());
        board.file = name.map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        Ok(())
    }

    /// Закрыть свой SolidWorks, если он простоял дольше `idle` и сейчас
    /// никому не нужен.
    fn close_idle(&self, idle: Duration) {
        if let Ok(mut session) = self.session.try_lock() {
            if session.idle().is_some_and(|quiet| quiet >= idle) {
                session.close();
                self.board().own = None;
            }
        }
    }
}

#[cfg(test)]
#[path = "hub_tests.rs"]
mod tests;
