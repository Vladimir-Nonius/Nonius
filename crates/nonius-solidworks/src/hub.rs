//! Один сеанс со своим SolidWorks на всю программу.
//!
//! Кто бы ни просил STEP или связи файлов — импорт, разбор папки, «Показать
//! в 3D», — все идут сюда и к одному и тому же SolidWorks: параллельно его
//! не запускают. Простоял без дела — закрывается сам, вежливо; понадобится
//! снова — поднимется.

use std::sync::{Mutex, Once, OnceLock};
use std::time::Duration;

use crate::session::{Failed, Hooks, Limits, Session};
use crate::wire::Command;
use crate::worker::PowerShell;

/// После стольких секунд без дела свой SolidWorks закрывается: он занимает
/// сотни мегабайт памяти.
const IDLE: Duration = Duration::from_secs(60);

/// Общий сеанс.
pub(crate) struct Hub {
    session: Mutex<Session>,
}

/// Сеанс этой программы.
pub(crate) fn hub() -> &'static Hub {
    static HUB: OnceLock<Hub> = OnceLock::new();
    static KEEPER: Once = Once::new();
    let hub = HUB.get_or_init(|| Hub {
        session: Mutex::new(Session::new(
            Box::new(PowerShell::default()),
            Limits::default(),
        )),
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

impl Hub {
    /// Выполнить команду в своём SolidWorks, дождавшись очереди.
    pub fn run(&self, command: &Command, hooks: &Hooks) -> Result<Vec<String>, Failed> {
        let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        session.run(command, hooks)
    }

    /// Закрыть свой SolidWorks, если он простоял дольше `idle` и сейчас
    /// никому не нужен.
    fn close_idle(&self, idle: Duration) {
        if let Ok(mut session) = self.session.try_lock() {
            if session.idle().is_some_and(|quiet| quiet >= idle) {
                session.close();
            }
        }
    }
}
