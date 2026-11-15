//! Очередь и состояние на поддельном рабочем процессе.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use super::*;
use crate::session::tests::{line, quick, run, Fake, Run, Say};

/// Ответ не сразу, но SolidWorks «считает»: пульс с растущим процессором.
fn slow(steps: u64) -> Vec<Say> {
    let mut says = Vec::new();
    for n in 0..steps {
        says.push(line(&format!("BEAT\t{}\t", n * 1500)));
        says.push(Say::Pause);
    }
    says.push(line("OK"));
    says
}

fn step(hub: &Hub, name: &str, ask: &Ask) -> Result<(), Failed> {
    let (source, target) = (Path::new(name), Path::new("model.step"));
    hub.run(&Command::Step { source, target }, ask).map(|_| ())
}

fn wait_for(what: &str, mut seen: impl FnMut() -> bool) {
    let began = Instant::now();
    while !seen() {
        assert!(began.elapsed().as_secs() < 20, "не дождались: {what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn human_goes_before_background_work() {
    // «а» работает долго; за это время приходят фоновая «б» и срочная «в».
    let (fake, log) = Fake::new(vec![run(|command| {
        if command.contains("а.sldprt") {
            slow(60)
        } else {
            vec![line("OK")]
        }
    })]);
    let limits = Limits {
        hard: Duration::from_secs(60),
        ..quick()
    };
    let hub = Hub::new(Session::new(fake, limits));
    let background = Ask {
        urgent: false,
        ..Ask::plain()
    };
    let sent = || log.lock().unwrap().sent.len();
    std::thread::scope(|scope| {
        scope.spawn(|| step(&hub, "а.sldprt", &background).unwrap());
        wait_for("«а» в работе", || sent() == 1);
        assert_eq!(
            hub.status(),
            Status::Working {
                file: "а.sldprt".into(),
                done: 1,
                total: 1
            }
        );
        scope.spawn(|| step(&hub, "б.sldprt", &background).unwrap());
        wait_for("«б» в очереди", || hub.board().background == 1);
        scope.spawn(|| step(&hub, "в.sldprt", &Ask::plain()).unwrap());
        wait_for("«в» в очереди", || hub.board().urgent == 1);
        // В очереди ещё двое: «1 из 3».
        assert!(matches!(
            hub.status(),
            Status::Working {
                done: 1,
                total: 3,
                ..
            }
        ));
    });
    let log = log.lock().unwrap();
    let order: Vec<&str> = log
        .sent
        .iter()
        .map(|l| l.split('\t').nth(1).unwrap())
        .collect();
    assert_eq!(order, ["а.sldprt", "в.sldprt", "б.sldprt"]);
    assert_eq!(log.launches, 1, "SolidWorks один на всех");
    assert_eq!(hub.status(), Status::Idle);
}

#[test]
fn status_tells_restart_and_place_in_the_batch() {
    // Запуск небыстрый: пока SolidWorks поднимается — «перезапускаюсь».
    let mut hello = vec![Say::Pause; 30];
    hello.push(line("READY\t100\t5\t33.5.0\tjob"));
    let starting = Run {
        hello,
        reply: Box::new(|_| slow(30)),
    };
    let (fake, _) = Fake::new(vec![starting]);
    let limits = Limits {
        start: Duration::from_secs(30),
        hard: Duration::from_secs(60),
        ..quick()
    };
    let hub = Hub::new(Session::new(fake, limits));
    let ask = Ask {
        urgent: false,
        place: Some((3, 27)),
        ..Ask::plain()
    };
    let (mut restarting, mut working) = (false, false);
    std::thread::scope(|scope| {
        let job = scope.spawn(|| step(&hub, "КТМ 640\\вал копия.sldprt", &ask));
        while !job.is_finished() {
            match hub.status() {
                Status::Restarting => restarting = true,
                Status::Working { file, done, total } => {
                    assert_eq!((file.as_str(), done, total), ("вал копия.sldprt", 3, 27));
                    working = true;
                }
                _ => {}
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        job.join().unwrap().unwrap();
    });
    assert!(restarting && working, "{restarting} {working}");
    assert_eq!(hub.status(), Status::Idle);
}

#[test]
fn waiting_in_the_queue_can_be_stopped() {
    let (fake, log) = Fake::new(vec![run(|_| slow(60))]);
    let limits = Limits {
        hard: Duration::from_secs(60),
        ..quick()
    };
    let hub = Hub::new(Session::new(fake, limits));
    let stop = AtomicBool::new(false);
    let notes = Mutex::new(Vec::new());
    let waiting = Ask {
        urgent: false,
        place: None,
        notify: &|s| notes.lock().unwrap().push(s.to_string()),
        cancelled: &|| stop.load(Ordering::SeqCst),
    };
    std::thread::scope(|scope| {
        scope.spawn(|| step(&hub, "а.sldprt", &Ask::plain()).unwrap());
        wait_for("«а» в работе", || {
            log.lock().unwrap().sent.len() == 1
        });
        let job = scope.spawn(|| step(&hub, "б.sldprt", &waiting));
        wait_for("«б» в очереди", || hub.board().background == 1);
        stop.store(true, Ordering::SeqCst);
        assert_eq!(job.join().unwrap(), Err(Failed::Cancelled));
    });
    assert_eq!(
        log.lock().unwrap().sent.len(),
        1,
        "«б» до SolidWorks не дошла"
    );
    assert_eq!(hub.board().background, 0);
}

#[test]
fn idle_solidworks_is_closed() {
    let (fake, log) = Fake::new(vec![run(|_| vec![line("OK")])]);
    let hub = Hub::new(Session::new(fake, quick()));
    step(&hub, "а.sldprt", &Ask::plain()).unwrap();
    hub.close_idle(Duration::from_secs(3600));
    assert!(!log
        .lock()
        .unwrap()
        .sent
        .contains(&crate::wire::QUIT.to_string()));
    hub.close_idle(Duration::ZERO);
    assert_eq!(log.lock().unwrap().sent.last().unwrap(), crate::wire::QUIT);
}
