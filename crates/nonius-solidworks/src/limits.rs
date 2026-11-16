//! Сроки надзора за своим SolidWorks и то, чем сеанс (`session.rs`)
//! обменивается с теми, кто его зовёт: почему команда не выполнена, чем
//! сеанс занят, куда писать строку хода.

use std::time::Duration;

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
    /// Перезапуск после стольких выгрузок STEP (открытых документов).
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
            restart_every: 50,
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
    pub notify: &'a (dyn Fn(&str) + Sync),
    /// Попросили ли остановить.
    pub cancelled: &'a (dyn Fn() -> bool + Sync),
    /// Сеанс сменил занятие.
    pub phase: &'a dyn Fn(Phase),
}
