//! Очередь к OpenCascade: одна тяжёлая работа за раз.
//!
//! OpenCascade падает при параллельном чтении STEP, поэтому сравнение,
//! разбор папки и импорт идут по одному. Но часть этих работ подолгу ждёт
//! SolidWorks (выгрузка STEP — секунды, а то и минуты) и OpenCascade в это
//! время не трогает. Чтобы 3D-сравнение не стояло за чужой выгрузкой STEP,
//! работа на время такого ожидания выходит из очереди ([`Gate::step_out`])
//! и встаёт в неё снова, когда SolidWorks ответил.
//!
//! Обычный `Mutex` тут не годится: его замок нельзя отдать и взять снова
//! из глубины работы, которая его держит.

use std::sync::{Condvar, Mutex, MutexGuard};

use nonius_gui::source::Progress;

/// Очередь.
#[derive(Default)]
pub(super) struct Gate {
    taken: Mutex<bool>,
    freed: Condvar,
}

/// Место в очереди занято, пока это значение живо.
pub(super) struct Held<'a>(&'a Gate);

impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.0.release();
    }
}

impl Gate {
    fn state(&self) -> MutexGuard<'_, bool> {
        self.taken.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Занять, если свободно.
    pub fn try_take(&self) -> Option<Held<'_>> {
        let mut taken = self.state();
        if *taken {
            return None;
        }
        *taken = true;
        Some(Held(self))
    }

    /// Дождаться и занять.
    pub fn take(&self) -> Held<'_> {
        self.enter();
        Held(self)
    }

    fn enter(&self) {
        let mut taken = self.state();
        while *taken {
            taken = self.freed.wait(taken).unwrap_or_else(|e| e.into_inner());
        }
        *taken = true;
    }

    fn release(&self) {
        *self.state() = false;
        self.freed.notify_one();
    }

    /// Тот, кто держит очередь, выходит из неё на время `wait` и встаёт
    /// снова. Если вернуться сразу не вышло (место заняли), `waiting`
    /// получает об этом знать — человек видит, чего ждёт работа.
    pub fn step_out(&self, wait: &mut dyn FnMut(), waiting: &dyn Fn()) {
        self.release();
        wait();
        if *self.state() {
            waiting();
        }
        self.enter();
    }
}

/// Ход работы ядра → строка окна, «Остановить» → отмена; на время ожидания
/// SolidWorks работа выходит из очереди к OpenCascade.
pub(super) struct Steps<'a> {
    /// Куда писать строку хода.
    pub progress: &'a dyn Progress,
    /// Очередь, которую держит эта работа.
    pub gate: &'a Gate,
    /// Что дописать к строке хода: у сравнения — многоточие.
    pub tail: &'static str,
}

impl nonius_app::Progress for Steps<'_> {
    fn step(&self, what: &str) {
        self.progress.step(&format!("{what}{}", self.tail));
    }

    fn cancelled(&self) -> bool {
        self.progress.cancelled()
    }

    fn outside(&self, wait: &mut dyn FnMut()) {
        self.gate
            .step_out(wait, &|| self.progress.step(super::WAIT));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn work_waiting_for_solidworks_lets_others_in() {
        let gate = Gate::default();
        let held = gate.take();
        assert!(gate.try_take().is_none(), "очередь занята");
        let (mut got_in, told) = (false, AtomicBool::new(false));
        // Пока держатель ждёт SolidWorks, другая работа проходит.
        gate.step_out(&mut || got_in = gate.try_take().is_some(), &|| {
            told.store(true, Ordering::SeqCst)
        });
        assert!(got_in);
        assert!(!told.load(Ordering::SeqCst), "место свободно — не ждали");
        // После ожидания очередь снова за держателем.
        assert!(gate.try_take().is_none());
        drop(held);
        assert!(gate.try_take().is_some());
    }

    #[test]
    fn returning_work_waits_for_the_one_that_got_in() {
        let gate = Gate::default();
        let _held = gate.take();
        let told = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let mut other = None;
            gate.step_out(
                &mut || {
                    // Другая работа заняла очередь и отпустит её чуть позже.
                    let inside = gate.take();
                    other = Some(scope.spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(30));
                        drop(inside);
                    }));
                },
                &|| told.store(true, Ordering::SeqCst),
            );
            other.unwrap().join().unwrap();
        });
        assert!(told.load(Ordering::SeqCst), "человеку сказано, чего ждём");
        assert!(gate.try_take().is_none(), "очередь снова наша");
    }
}
