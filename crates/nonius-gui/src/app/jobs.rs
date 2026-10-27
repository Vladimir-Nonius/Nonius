//! Долгая работа в фоне: окно не замирает, пока читаются версии и считается
//! сравнение. Каждая работа — свой поток; результат приходит в окно сообщением.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::source::{Comparison, PartEntry, Pick, ProjectSource, VersionEntry};

/// Готовый результат.
pub enum Done {
    /// Проект открыт.
    Opened(anyhow::Result<Box<dyn ProjectSource>>),
    /// Список деталей.
    Parts(anyhow::Result<Vec<PartEntry>>),
    /// Версии детали.
    Versions(String, anyhow::Result<Vec<VersionEntry>>),
    /// Сравнение: деталь, пара версий, результат.
    Compared(String, (Pick, Pick), anyhow::Result<Comparison>),
    /// Работа упала с внутренней ошибкой.
    Crashed,
}

/// Очередь фоновых работ.
pub struct Jobs {
    tx: Sender<Done>,
    rx: Receiver<Done>,
    running: usize,
    progress: Arc<Mutex<String>>,
}

impl Default for Jobs {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            tx,
            rx,
            running: 0,
            progress: Arc::default(),
        }
    }
}

impl Jobs {
    /// Запустить работу. `work` получает функцию для сообщений о ходе работы.
    pub fn spawn(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce(&dyn Fn(&str)) -> Done + Send + 'static,
    ) {
        self.running += 1;
        let (tx, ctx, progress) = (self.tx.clone(), ctx.clone(), self.progress.clone());
        std::thread::spawn(move || {
            let report = |text: &str| {
                if let Ok(mut p) = progress.lock() {
                    *p = text.to_string();
                }
                ctx.request_repaint();
            };
            // Паника в работе не должна оставить окно в вечном ожидании.
            let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&report)));
            let _ = tx.send(done.unwrap_or(Done::Crashed));
            ctx.request_repaint();
        });
    }

    /// Забрать готовые результаты.
    pub fn finished(&mut self) -> Vec<Done> {
        let done: Vec<Done> = self.rx.try_iter().collect();
        self.running = self.running.saturating_sub(done.len());
        if self.running == 0 {
            if let Ok(mut p) = self.progress.lock() {
                p.clear();
            }
        }
        done
    }

    /// Идёт ли работа.
    pub fn busy(&self) -> bool {
        self.running > 0
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
            p("читаю");
            Done::Parts(Ok(Vec::new()))
        });
        jobs.spawn(&ctx, |_| panic!("сбой"));
        assert!(jobs.busy());
        let mut got = Vec::new();
        for _ in 0..200 {
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
}
