//! Тесты сеанса на поддельном рабочем процессе: ни PowerShell, ни
//! SolidWorks. Подделка отвечает на команды по сценарию и помнит, что с
//! ней делали.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::*;

/// Что подделка отдаёт на очередной вопрос «есть ли строка».
#[derive(Clone)]
pub(crate) enum Say {
    Line(String),
    /// Помолчать один шаг ожидания.
    Pause,
    /// Закрыться.
    Close,
}

pub(crate) fn line(text: &str) -> Say {
    Say::Line(text.to_string())
}

/// Сценарий одного запуска: что сказать при старте и что — на команду.
pub(crate) type Reply = Box<dyn FnMut(&str) -> Vec<Say> + Send>;

pub(crate) struct Run {
    pub hello: Vec<Say>,
    pub reply: Reply,
}

/// Обычный запуск: готов, на команды отвечает `reply`.
pub(crate) fn run(reply: impl FnMut(&str) -> Vec<Say> + Send + 'static) -> Run {
    Run {
        hello: vec![line("READY\t100\t5\t33.5.0\tjob")],
        reply: Box::new(reply),
    }
}

/// Что происходило с подделками.
#[derive(Default)]
pub(crate) struct Log {
    pub launches: usize,
    pub sent: Vec<String>,
    pub kills: usize,
}

/// Поддельный способ запуска: каждый запуск берёт следующий сценарий.
pub(crate) struct Fake {
    runs: Mutex<VecDeque<Run>>,
    pub log: Arc<Mutex<Log>>,
}

impl Fake {
    pub fn new(runs: Vec<Run>) -> (Box<Self>, Arc<Mutex<Log>>) {
        let log = Arc::new(Mutex::new(Log::default()));
        let fake = Self {
            runs: Mutex::new(runs.into()),
            log: log.clone(),
        };
        (Box::new(fake), log)
    }
}

struct FakeWorker {
    queue: VecDeque<Say>,
    reply: Reply,
    closed: bool,
    log: Arc<Mutex<Log>>,
}

impl Launch for Fake {
    fn launch(&self) -> Result<Box<dyn Worker>, String> {
        self.log.lock().unwrap().launches += 1;
        let run = self.runs.lock().unwrap().pop_front();
        let run = run.ok_or("SolidWorks не установлен")?;
        Ok(Box::new(FakeWorker {
            queue: run.hello.into(),
            reply: run.reply,
            closed: false,
            log: self.log.clone(),
        }))
    }
}

impl Worker for FakeWorker {
    fn send(&mut self, text: &str) -> bool {
        if self.closed {
            return false;
        }
        self.log.lock().unwrap().sent.push(text.to_string());
        if text == QUIT {
            self.queue.push_back(Say::Close);
        } else {
            self.queue.extend((self.reply)(text));
        }
        true
    }

    fn next(&mut self, wait: Duration) -> Got {
        if self.closed {
            return Got::Closed;
        }
        match self.queue.pop_front() {
            Some(Say::Line(text)) => Got::Line(text),
            Some(Say::Close) => {
                self.closed = true;
                Got::Closed
            }
            Some(Say::Pause) | None => {
                std::thread::sleep(wait);
                Got::Quiet
            }
        }
    }

    fn kill(&mut self) {
        if !self.closed {
            self.log.lock().unwrap().kills += 1;
        }
        self.closed = true;
    }
}

/// Сроки в миллисекундах: тесты не ждут.
pub(crate) fn quick() -> Limits {
    Limits {
        start: Duration::from_millis(200),
        stall: Duration::from_millis(60),
        dialog: Duration::from_millis(20),
        hard: Duration::from_millis(400),
        work_ms: 1000,
        busy_tries: 3,
        busy_pause: Duration::from_millis(1),
        restart_every: 40,
        rest: Duration::from_secs(60),
        farewell: Duration::from_millis(50),
        tick: Duration::from_millis(2),
    }
}

fn hooks<'a>(notes: &'a Mutex<Vec<String>>, stop: &'a dyn Fn() -> bool) -> Hooks<'a> {
    // Строки хода складываются в `notes` через «утечку» замыкания: в тесте
    // это проще, чем тащить время жизни.
    let notify: &'a dyn Fn(&str) = Box::leak(Box::new(move |s: &str| {
        notes.lock().unwrap().push(s.to_string())
    }));
    Hooks {
        notify,
        cancelled: stop,
        phase: &no_phase,
    }
}

fn no_phase(_: Phase) {}

fn step(session: &mut Session, name: &str, notes: &Mutex<Vec<String>>) -> Result<(), Failed> {
    let (source, target) = (Path::new(name), Path::new("model.step"));
    let command = Command::Step { source, target };
    session.run(&command, &hooks(notes, &|| false)).map(|_| ())
}

const DEAD: &str = "ERROR\t0x800706BA\tСервер RPC недоступен";
const BUSY: &str = "ERROR\t0x8001010A\tприложение занято";

#[test]
fn one_worker_serves_all_commands() {
    let (fake, log) = Fake::new(vec![run(|command| {
        if command.starts_with("deps") {
            vec![line("DEP\tC:\\п\\КТМ 640\\вал.SLDPRT"), line("OK")]
        } else {
            vec![line("BEAT\t10\t"), line("OK")]
        }
    })]);
    let mut session = Session::new(fake, quick());
    let notes = Mutex::new(Vec::new());
    step(&mut session, "вал.sldprt", &notes).unwrap();
    step(&mut session, "КТМ 640\\ось копия.sldprt", &notes).unwrap();
    let deps = Command::Deps(Path::new("узел.sldasm"));
    let got = session.run(&deps, &hooks(&notes, &|| false)).unwrap();
    assert_eq!(got, ["C:\\п\\КТМ 640\\вал.SLDPRT"]);
    session
        .run(&Command::Ping, &hooks(&notes, &|| false))
        .unwrap();
    let log = log.lock().unwrap();
    assert_eq!(log.launches, 1, "PowerShell и SolidWorks запущены один раз");
    assert_eq!(log.sent.len(), 4);
    assert!(log.sent[1].contains("КТМ 640\\ось копия.sldprt"));
    assert_eq!(session.own().unwrap().pid, 100);
    assert!(notes.lock().unwrap().is_empty());
}

#[test]
fn refusals_final_and_passing() {
    let (fake, log) = Fake::new(vec![run(|command| {
        if command.contains("битая") {
            vec![line("ERROR\t1\tSolidWorks не открыл файл (код 8192)")]
        } else {
            vec![line("ERROR\t0\tSolidWorks не выгрузил STEP (код 1)")]
        }
    })]);
    let mut session = Session::new(fake, quick());
    let notes = Mutex::new(Vec::new());
    assert_eq!(
        step(&mut session, "битая.sldprt", &notes),
        Err(Failed::Refused(
            "SolidWorks не открыл файл (код 8192)".into()
        ))
    );
    assert!(matches!(
        step(&mut session, "вал.sldprt", &notes),
        Err(Failed::Later(_))
    ));
    // SolidWorks жив и ответил: перезапускать незачем.
    assert_eq!(log.lock().unwrap().launches, 1);
    assert_eq!(log.lock().unwrap().kills, 0);
}

#[test]
fn hung_solidworks_is_restarted_and_the_command_repeated() {
    // Первый запуск молчит (процессор стоит), второй отвечает.
    let (fake, log) = Fake::new(vec![
        run(|_| vec![line("BEAT\t500\t"), line("BEAT\t500\t")]),
        run(|_| vec![line("OK")]),
    ]);
    let mut session = Session::new(fake, quick());
    let notes = Mutex::new(Vec::new());
    step(&mut session, "вал.sldprt", &notes).unwrap();
    let log = log.lock().unwrap();
    assert_eq!((log.launches, log.kills), (2, 1));
    assert_eq!(log.sent.len(), 2, "команда повторена новому процессу");
    assert_eq!(*notes.lock().unwrap(), [RESTARTING]);
}

#[test]
fn file_that_kills_solidworks_twice_is_refused_and_work_goes_on() {
    let (fake, log) = Fake::new(vec![
        run(|_| vec![line(DEAD)]),
        run(|_| vec![line("GONE")]),
        run(|_| vec![line("OK")]),
    ]);
    let mut session = Session::new(fake, quick());
    let notes = Mutex::new(Vec::new());
    let Err(Failed::Refused(why)) = step(&mut session, "убийца.sldprt", &notes) else {
        panic!("файл должен получить окончательный отказ");
    };
    assert!(why.contains("дважды не справился"), "{why}");
    // Следующая деталь идёт своим чередом, на новом SolidWorks.
    step(&mut session, "вал.sldprt", &notes).unwrap();
    assert_eq!(log.lock().unwrap().launches, 3);
}

#[test]
fn busy_solidworks_gets_the_command_again() {
    let mut asked = 0;
    let (fake, log) = Fake::new(vec![run(move |_| {
        asked += 1;
        vec![line(if asked < 3 { BUSY } else { "OK" })]
    })]);
    let mut session = Session::new(fake, quick());
    let notes = Mutex::new(Vec::new());
    step(&mut session, "вал.sldprt", &notes).unwrap();
    assert_eq!(log.lock().unwrap().sent.len(), 3);
    assert_eq!(log.lock().unwrap().launches, 1, "занят — не умер");

    // Занят всё время — «сейчас не вышло», без пометки файла.
    let (fake, _) = Fake::new(vec![run(|_| vec![line(BUSY)])]);
    let mut session = Session::new(fake, quick());
    assert!(matches!(
        step(&mut session, "вал.sldprt", &notes),
        Err(Failed::Later(_))
    ));
}

#[test]
fn working_solidworks_is_given_time() {
    // Ответ приходит позже срока «завис», но процессор всё время занят.
    let (fake, log) = Fake::new(vec![run(|_| {
        let mut says = Vec::new();
        for n in 0..40 {
            says.push(line(&format!("BEAT\t{}\t", n * 1500)));
            says.push(Say::Pause);
        }
        says.push(line("OK"));
        says
    })]);
    let limits = Limits {
        hard: Duration::from_secs(30),
        ..quick()
    };
    let mut session = Session::new(fake, limits);
    let notes = Mutex::new(Vec::new());
    step(&mut session, "коллектор.sldprt", &notes).unwrap();
    assert_eq!(log.lock().unwrap().launches, 1);
}

#[test]
fn windows_of_own_solidworks_are_noticed() {
    // Отчёт об ошибке — упал сразу, не дожидаясь срока.
    let crash = || run(|_| vec![line("BEAT\t10\tSOLIDWORKS Error Report")]);
    // Вопрос в окне при стоящем процессоре — ответить некому.
    let question = || {
        run(|_| {
            let beat = line("BEAT\t10\tSOLIDWORKS");
            vec![beat.clone(), Say::Pause, beat.clone(), Say::Pause, beat]
        })
    };
    let (fake, _) = Fake::new(vec![crash(), question()]);
    let mut session = Session::new(fake, quick());
    let notes = Mutex::new(Vec::new());
    let began = Instant::now();
    let Err(Failed::Refused(why)) = step(&mut session, "вал.sldprt", &notes) else {
        panic!("два сбоя подряд — отказ");
    };
    assert!(why.contains("ждёт ответа в окне «SOLIDWORKS»"), "{why}");
    assert!(began.elapsed() < quick().stall * 3);
}

#[test]
fn restart_after_so_many_commands() {
    let limits = Limits {
        restart_every: 2,
        ..quick()
    };
    let ok = || run(|_| vec![line("OK")]);
    let (fake, log) = Fake::new(vec![ok(), ok()]);
    let mut session = Session::new(fake, limits);
    let notes = Mutex::new(Vec::new());
    for name in ["а.sldprt", "б.sldprt", "в.sldprt"] {
        step(&mut session, name, &notes).unwrap();
    }
    let log = log.lock().unwrap();
    assert_eq!(log.launches, 2);
    assert_eq!(log.kills, 0, "перезапуск по счётчику — вежливый");
    assert_eq!(log.sent[2], QUIT);
    assert_eq!(*notes.lock().unwrap(), [RESTARTING]);
}

#[test]
fn failed_start_is_not_retried_at_once() {
    let broken = Run {
        hello: vec![line("ERROR\t0\tSolidWorks слишком долго запускается")],
        reply: Box::new(|_| Vec::new()),
    };
    let (fake, log) = Fake::new(vec![broken, run(|_| vec![line("OK")])]);
    let mut session = Session::new(fake, quick());
    let notes = Mutex::new(Vec::new());
    for _ in 0..3 {
        let Err(Failed::Later(why)) = step(&mut session, "вал.sldprt", &notes) else {
            panic!("запуск не удался — «сейчас не вышло»");
        };
        assert!(why.contains("слишком долго"), "{why}");
    }
    assert_eq!(
        log.lock().unwrap().launches,
        1,
        "вся папка не ждёт по три минуты"
    );
    assert!(session.own().is_none());
}

#[test]
fn stopped_command_is_waited_out_before_the_next_one() {
    // На первую команду ответ придёт не сразу; человек жмёт «Остановить».
    let mut asked = 0;
    let (fake, log) = Fake::new(vec![run(move |_| {
        asked += 1;
        if asked == 1 {
            vec![Say::Pause, Say::Pause, Say::Pause, line("OK")]
        } else {
            vec![line("DEP\tвторой ответ"), line("OK")]
        }
    })]);
    let mut session = Session::new(fake, quick());
    let notes = Mutex::new(Vec::new());
    let (source, target) = (Path::new("вал.sldprt"), Path::new("model.step"));
    let command = Command::Step { source, target };
    // Остановили во время работы (команда уже ушла): SolidWorks не
    // снимается…
    let sent = log.clone();
    let stop = move || sent.lock().unwrap().sent.len() == 1;
    let stopped = session.run(&command, &hooks(&notes, &stop));
    assert_eq!(stopped, Err(Failed::Cancelled));
    assert_eq!(log.lock().unwrap().kills, 0);
    // …а следующая команда сначала дожидается ответа на прошлую и чужой
    // «OK» за свой не принимает.
    let deps = Command::Deps(Path::new("узел.sldasm"));
    let got = session.run(&deps, &hooks(&notes, &|| false)).unwrap();
    assert_eq!(got, ["второй ответ"]);
    assert_eq!(log.lock().unwrap().launches, 1);
}

#[test]
fn idle_solidworks_can_be_closed_politely() {
    let (fake, log) = Fake::new(vec![run(|_| vec![line("OK")]), run(|_| vec![line("OK")])]);
    let mut session = Session::new(fake, quick());
    let notes = Mutex::new(Vec::new());
    assert_eq!(session.idle(), None);
    step(&mut session, "вал.sldprt", &notes).unwrap();
    assert!(session.idle().is_some());
    session.close();
    assert_eq!(session.idle(), None);
    assert_eq!(log.lock().unwrap().sent.last().unwrap(), QUIT);
    assert_eq!(log.lock().unwrap().kills, 0);
    // Понадобился снова — поднимается заново.
    step(&mut session, "вал.sldprt", &notes).unwrap();
    assert_eq!(log.lock().unwrap().launches, 2);
}
