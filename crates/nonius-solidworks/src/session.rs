//! Сеанс со своим SolidWorks: один долгоживущий рабочий процесс, команды
//! по одной, и надзор за каждой.
//!
//! У COM-вызова своего срока нет: зависший SolidWorks держит его вечно.
//! Поэтому срок считается здесь ([`Limits`]), по «пульсу» рабочего процесса:
//! - время процессора своего SolidWorks не растёт — он **завис** (растёт —
//!   просто считает, ждём дальше, до общего предела);
//! - у него появилось окно отчёта об ошибке — он **упал**; другое
//!   окно-сообщение при стоящем процессоре — ждёт человека, которого нет;
//! - код COM «умер» — упал; «занят» — та же команда повторяется чуть позже.
//!
//! Упал или завис — рабочий процесс снимается (свой SolidWorks Windows
//! закрывает следом), поднимается новый, и команда повторяется. Файл,
//! который уронил SolidWorks два раза подряд, получает окончательный отказ
//! ([`Failed::Refused`]) — склад STEP его запомнит, и больше его не пробуют.
//! Остальная работа идёт дальше: уже посчитанное лежит на складе.
//!
//! После каждых [`Limits::restart_every`] команд SolidWorks перезапускается
//! вежливо: за долгую пачку он копит память.

use std::time::{Duration, Instant};

use crate::wire::{crash_report, fault, parse, Command, Fault, Line, Own, QUIT};
use crate::worker::{Got, Launch, Worker};

/// Строка хода, пока свой SolidWorks поднимается заново.
pub(crate) const RESTARTING: &str = "SolidWorks перезапускается…";

/// Сроки и счётчики надзора.
#[derive(Debug, Clone)]
pub(crate) struct Limits {
    /// Сколько ждать запуска SolidWorks.
    pub start: Duration,
    /// Столько без работы процессора — завис.
    pub stall: Duration,
    /// Столько с окном-сообщением и без работы процессора — ждёт человека.
    pub dialog: Duration,
    /// Больше этого над одной командой не работаем, даже если считает.
    pub hard: Duration,
    /// Меньше этого времени процессора за срок — «не работает», мс.
    pub work_ms: u64,
    /// Сколько раз повторять команду, пока SolidWorks «занят».
    pub busy_tries: usize,
    /// Пауза перед повтором.
    pub busy_pause: Duration,
    /// Перезапуск после стольких команд.
    pub restart_every: usize,
    /// После неудачного запуска столько не пробуем снова.
    pub rest: Duration,
    /// Сколько ждать вежливого закрытия.
    pub farewell: Duration,
    /// Шаг ожидания строки (как часто проверять сроки и «Остановить»).
    pub tick: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            start: Duration::from_secs(180),
            stall: Duration::from_secs(90),
            dialog: Duration::from_secs(20),
            hard: Duration::from_secs(20 * 60),
            work_ms: 1000,
            busy_tries: 5,
            busy_pause: Duration::from_secs(2),
            restart_every: 40,
            rest: Duration::from_secs(60),
            farewell: Duration::from_secs(25),
            tick: Duration::from_millis(200),
        }
    }
}

/// Почему команда не выполнена.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failed {
    /// Окончательно: с этим файлом не выйдет, пока он тот же.
    Refused(String),
    /// Сейчас не вышло: SolidWorks не запустился, занят, не установлен.
    Later(String),
    /// Человек нажал «Остановить».
    Cancelled,
}

/// Чем занят сеанс — для состояния наружу.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Выполняет команду.
    Working,
    /// Поднимает SolidWorks (первый раз или заново).
    Restarting,
}

/// Связь с тем, кто просит: строка хода, «Остановить», состояние.
pub(crate) struct Hooks<'a> {
    /// Строка хода для человека.
    pub notify: &'a dyn Fn(&str),
    /// Попросили ли остановить.
    pub cancelled: &'a dyn Fn() -> bool,
    /// Сеанс сменил занятие.
    pub phase: &'a dyn Fn(Phase),
}

/// Чем кончилась одна попытка команды.
enum Outcome {
    Done(Vec<String>),
    Said {
        code: u32,
        why: String,
    },
    /// Упал, завис, закрылся.
    Lost(String),
    Cancelled,
}

/// Сеанс: рабочий процесс (если запущен) и счётчики.
pub(crate) struct Session {
    launch: Box<dyn Launch>,
    limits: Limits,
    worker: Option<Box<dyn Worker>>,
    own: Option<Own>,
    /// Сколько команд выполнил этот рабочий процесс.
    served: usize,
    /// Остановленная команда ещё не ответила: её ответ надо дождаться.
    pending: bool,
    /// Запуск не удался: до этого времени не пробуем, причина та же.
    resting: Option<(Instant, String)>,
    used: Instant,
}

impl Session {
    pub fn new(launch: Box<dyn Launch>, limits: Limits) -> Self {
        Self {
            launch,
            limits,
            worker: None,
            own: None,
            served: 0,
            pending: false,
            resting: None,
            used: Instant::now(),
        }
    }

    /// Свой SolidWorks, если он сейчас запущен.
    #[allow(dead_code)] // понадобится состоянию наружу
    pub fn own(&self) -> Option<&Own> {
        self.worker.as_ref().and(self.own.as_ref())
    }

    /// Сколько свой SolidWorks простаивает. `None` — он не запущен.
    pub fn idle(&self) -> Option<Duration> {
        self.worker.as_ref().map(|_| self.used.elapsed())
    }

    /// Выполнить команду. Ответ — строки данных.
    pub fn run(&mut self, command: &Command, hooks: &Hooks) -> Result<Vec<String>, Failed> {
        let line = command.line().map_err(Failed::Refused)?;
        let (mut crashes, mut busy) = (0, 0);
        loop {
            if (hooks.cancelled)() {
                return Err(Failed::Cancelled);
            }
            self.ready(hooks)?;
            (hooks.phase)(Phase::Working);
            let outcome = self.ask(&line, hooks);
            self.used = Instant::now();
            let why = match outcome {
                Outcome::Done(data) => {
                    self.served += 1;
                    return Ok(data);
                }
                Outcome::Cancelled => return Err(Failed::Cancelled),
                Outcome::Said { code, why } => match fault(code) {
                    Fault::Final => {
                        self.served += 1;
                        return Err(Failed::Refused(why));
                    }
                    Fault::Plain => {
                        self.served += 1;
                        return Err(Failed::Later(why));
                    }
                    Fault::Busy => {
                        busy += 1;
                        if busy >= self.limits.busy_tries {
                            return Err(Failed::Later(format!("SolidWorks занят: {why}")));
                        }
                        std::thread::sleep(self.limits.busy_pause);
                        continue;
                    }
                    Fault::Dead => format!("SolidWorks упал ({why})"),
                },
                Outcome::Lost(why) => why,
            };
            // Упал или завис: снять и поднять заново.
            self.drop_worker();
            crashes += 1;
            if crashes >= 2 {
                return Err(Failed::Refused(format!(
                    "SolidWorks дважды не справился с этим файлом: {why}"
                )));
            }
            (hooks.notify)(RESTARTING);
        }
    }

    /// Вежливо закрыть свой SolidWorks: попросить и подождать; не ушёл —
    /// снять.
    pub fn close(&mut self) {
        let Some(mut worker) = self.worker.take() else {
            return;
        };
        self.pending = false;
        if worker.send(QUIT) {
            let deadline = Instant::now() + self.limits.farewell;
            while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                if worker.next(left.min(self.limits.tick.max(Duration::from_millis(1))))
                    == Got::Closed
                {
                    return;
                }
            }
        }
        worker.kill();
    }

    fn drop_worker(&mut self) {
        if let Some(mut worker) = self.worker.take() {
            worker.kill();
        }
        self.pending = false;
    }

    /// Рабочий процесс запущен, свободен и не засиделся.
    fn ready(&mut self, hooks: &Hooks) -> Result<(), Failed> {
        if self.pending {
            // Прошлую команду остановили, а SolidWorks её ещё делает.
            match self.listen(hooks) {
                Outcome::Cancelled => return Err(Failed::Cancelled),
                Outcome::Lost(_) => self.drop_worker(),
                Outcome::Done(_) | Outcome::Said { .. } => self.pending = false,
            }
        }
        if self.worker.is_some() && self.served >= self.limits.restart_every {
            (hooks.phase)(Phase::Restarting);
            (hooks.notify)(RESTARTING);
            self.close();
        }
        if self.worker.is_none() {
            (hooks.phase)(Phase::Restarting);
            self.start(hooks)?;
        }
        Ok(())
    }

    fn start(&mut self, hooks: &Hooks) -> Result<(), Failed> {
        if let Some((until, why)) = &self.resting {
            if Instant::now() < *until {
                return Err(Failed::Later(why.clone()));
            }
        }
        self.resting = None;
        let rest = self.limits.rest;
        let mut fail = |why: String| {
            self.resting = Some((Instant::now() + rest, why.clone()));
            Err(Failed::Later(why))
        };
        let mut worker = match self.launch.launch() {
            Ok(worker) => worker,
            Err(why) => return fail(why),
        };
        let began = Instant::now();
        loop {
            if (hooks.cancelled)() {
                worker.kill();
                return Err(Failed::Cancelled);
            }
            if began.elapsed() > self.limits.start {
                worker.kill();
                return fail("SolidWorks слишком долго запускается".into());
            }
            match worker.next(self.limits.tick) {
                Got::Quiet => {}
                Got::Closed => {
                    return fail("SolidWorks не запустился: рабочий процесс закрылся".into())
                }
                Got::Line(text) => match parse(&text) {
                    Line::Ready(own) => {
                        self.own = Some(own);
                        break;
                    }
                    Line::Error { why, .. } => {
                        worker.kill();
                        return fail(why);
                    }
                    _ => {}
                },
            }
        }
        self.worker = Some(worker);
        self.served = 0;
        self.used = Instant::now();
        Ok(())
    }

    /// Отправить команду и дождаться ответа.
    fn ask(&mut self, line: &str, hooks: &Hooks) -> Outcome {
        let Some(worker) = self.worker.as_mut() else {
            return Outcome::Lost("рабочий процесс SolidWorks не запущен".into());
        };
        // Старый пульс, накопившийся за простой, к этой команде не относится.
        loop {
            match worker.next(Duration::ZERO) {
                Got::Line(_) => {}
                Got::Quiet => break,
                Got::Closed => return Outcome::Lost("рабочий процесс SolidWorks закрылся".into()),
            }
        }
        if !worker.send(line) {
            return Outcome::Lost("рабочий процесс SolidWorks закрылся".into());
        }
        self.listen(hooks)
    }

    /// Ждать ответа на отправленную команду, следя за пульсом.
    fn listen(&mut self, hooks: &Hooks) -> Outcome {
        let limits = self.limits.clone();
        let Some(worker) = self.worker.as_mut() else {
            return Outcome::Lost("рабочий процесс SolidWorks не запущен".into());
        };
        let began = Instant::now();
        // Когда и при каком счётчике процессора SolidWorks последний раз
        // «работал» (потратил ещё `work_ms`).
        let (mut worked, mut spent) = (began, None::<u64>);
        let mut dialog: Option<(String, Instant)> = None;
        let mut data = Vec::new();
        loop {
            let got = worker.next(limits.tick);
            let now = Instant::now();
            match got {
                Got::Quiet => {}
                Got::Closed => return Outcome::Lost("рабочий процесс SolidWorks закрылся".into()),
                Got::Line(text) => match parse(&text) {
                    Line::Ok => return Outcome::Done(data),
                    Line::Error { code, why } => return Outcome::Said { code, why },
                    Line::Dep(file) => data.push(file),
                    Line::Gone => return Outcome::Lost("SolidWorks закрылся сам".into()),
                    Line::Beat {
                        cpu_ms,
                        dialog: title,
                    } => {
                        match spent {
                            None => spent = Some(cpu_ms),
                            Some(before) if cpu_ms >= before + limits.work_ms => {
                                (worked, spent) = (now, Some(cpu_ms));
                            }
                            Some(_) => {}
                        }
                        match title {
                            Some(title) if crash_report(&title) => {
                                return Outcome::Lost(format!("SolidWorks упал (окно «{title}»)"));
                            }
                            Some(title) => {
                                if dialog.as_ref().is_none_or(|(shown, _)| *shown != title) {
                                    dialog = Some((title, now));
                                }
                            }
                            None => dialog = None,
                        }
                    }
                    Line::Ready(_) | Line::Other => {}
                },
            }
            if (hooks.cancelled)() {
                self.pending = true;
                return Outcome::Cancelled;
            }
            if let Some((title, since)) = &dialog {
                if now.duration_since(worked.max(*since)) > limits.dialog {
                    return Outcome::Lost(format!(
                        "SolidWorks ждёт ответа в окне «{title}», а ответить некому"
                    ));
                }
            }
            if now.duration_since(worked) > limits.stall {
                return Outcome::Lost("SolidWorks завис: не отвечает и ничего не считает".into());
            }
            if now.duration_since(began) > limits.hard {
                let minutes = limits.hard.as_secs() / 60;
                return Outcome::Lost(format!(
                    "SolidWorks работал над файлом дольше {minutes} мин"
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
