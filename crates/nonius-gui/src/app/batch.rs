//! Очередь: перенести или удалить несколько предметов одной фоновой работой.
//! Предметы идут по одному, в строке состояния — «Переношу 3 из 5: …» и
//! кнопка «Остановить»: она останавливает после текущего. Итог — одной
//! строкой, ошибки — по строке на предмет. Одна операция говорит прежними
//! словами: ««корпус» перенесена в «Стена»».

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui;

use super::jobs::Done;
use super::node::Node;
use super::tree_folders::last;
use super::App;
use crate::source::ProjectSource;
use crate::texts::texts;

/// Что сделать с одним предметом.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Op {
    /// Перенести в папку (пустая строка — корень).
    Move(Node, String),
    /// Удалить: деталь — в «Удалённые детали», файл папки — из папки.
    Delete(Node),
}

impl Op {
    fn node(&self) -> &Node {
        match self {
            Op::Move(node, _) | Op::Delete(node) => node,
        }
    }

    /// Ход работы: «Переношу», «Удаляю».
    fn doing(&self) -> &'static str {
        match self {
            Op::Move(..) => texts().moving,
            Op::Delete(_) => texts().deleting,
        }
    }

    /// Итог: «Перенесено», «Удалено».
    fn done(&self) -> &'static str {
        match self {
            Op::Move(..) => texts().moved_many,
            Op::Delete(_) => texts().deleted_many,
        }
    }

    /// Сделать. Получилось — где предмет теперь (`None` — его больше нет).
    fn run(&self, project: &dyn ProjectSource) -> anyhow::Result<Option<Node>> {
        match self {
            Op::Move(Node::Part(part), folder) => {
                project.move_part(part, folder)?;
                Ok(Some(Node::Part(part.clone())))
            }
            Op::Delete(Node::Part(part)) => project.delete_part(part).map(|()| None),
            Op::Delete(Node::File(path)) => project.remove_folder_file(path).map(|()| None),
            Op::Move(..) => anyhow::bail!(texts().cant_move_this),
            Op::Delete(Node::Folder(_)) => anyhow::bail!(texts().cant_delete_folder),
        }
    }
}

/// Имя предмета для людей: деталь — имя, файл и папка — последнее имя.
fn name(node: &Node) -> &str {
    match node {
        Node::Part(name) => name,
        Node::Folder(path) | Node::File(path) => last(path),
    }
}

/// Идущая очередь.
pub(super) struct Batch {
    /// Флажок «Остановить»: работа смотрит его перед каждым предметом.
    stop: Arc<AtomicBool>,
}

impl Batch {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Итог очереди: что с каждым предметом и остановили ли её.
pub struct Report {
    results: Vec<(Op, anyhow::Result<Option<Node>>)>,
    total: usize,
    stopped: bool,
}

impl Report {
    /// «Перенесено 3 из 5», «Удалено 1 из 3, остановлено».
    fn summary(&self) -> String {
        let t = texts();
        let verb = self
            .results
            .first()
            .map_or(t.deleted_many, |(op, _)| op.done());
        let ok = self.results.iter().filter(|(_, r)| r.is_ok()).count();
        let mut text = format!("{verb} {ok} {} {}", t.of, self.total);
        if self.stopped {
            text = format!("{text}, {}", t.stopped);
        }
        text
    }

    /// Ошибки по строке на предмет.
    fn errors(&self) -> Option<String> {
        let lines: Vec<String> = self
            .results
            .iter()
            .filter_map(|(op, r)| {
                let e = r.as_ref().err()?;
                Some(format!("«{}»: {e:#}", name(op.node())))
            })
            .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }
}

impl App {
    /// Запустить очередь одной фоновой работой. Не отменяется выбором
    /// другой детали: её останавливает только «Остановить».
    pub(super) fn run_batch(&mut self, ctx: &egui::Context, ops: Vec<Op>) {
        let Some(project) = self.tab.project.clone() else {
            return;
        };
        if ops.is_empty() {
            return;
        }
        if self.tab.batch.is_some() {
            self.tab.notice = Some(texts().batch_busy.to_string());
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        self.tab.batch = Some(Batch { stop: stop.clone() });
        self.tab.jobs.spawn(ctx, move |progress| {
            let total = ops.len();
            let mut results = Vec::new();
            let mut stopped = false;
            for (i, op) in ops.into_iter().enumerate() {
                if stop.load(Ordering::Relaxed) {
                    stopped = true;
                    break;
                }
                let t = texts();
                let what = name(op.node());
                progress.step(&format!(
                    "{} {} {} {total}: {what}",
                    op.doing(),
                    i + 1,
                    t.of
                ));
                let result = op.run(project.as_ref());
                results.push((op, result));
            }
            Done::Batch(Report {
                results,
                total,
                stopped,
            })
        });
    }

    /// Очередь закончилась: показать итог, выделить получившееся,
    /// перечитать проект один раз.
    pub(super) fn batch_done(&mut self, ctx: &egui::Context, report: Report) {
        self.tab.batch = None;
        if let [(op, result)] = report.results.as_slice() {
            if report.total == 1 {
                self.one_done(ctx, op, result);
                return;
            }
        }
        let mut now = Vec::new();
        for (op, result) in &report.results {
            match (op, result) {
                (Op::Delete(Node::Part(name)), Ok(_)) => self.forget_part(name),
                (Op::Move(_, folder), Ok(Some(node))) => {
                    self.unfold_to(folder);
                    now.push(node.clone());
                }
                _ => {}
            }
        }
        if !now.is_empty() {
            self.tab.selection.clear();
            for node in now {
                self.tab.selection.toggle(node);
            }
        }
        // Перечитанный список может сам открыть деталь, а выбор стирает
        // строку: итог вернётся, когда деталь покажется.
        self.tab.notice = Some(report.summary());
        self.tab.sticky = Some(report.summary());
        self.tab.message = report.errors();
        self.reload(ctx);
    }

    /// Одна операция: прежние слова.
    fn one_done(&mut self, ctx: &egui::Context, op: &Op, result: &anyhow::Result<Option<Node>>) {
        let t = texts();
        match (op, result) {
            (_, Err(e)) => self.tab.message = Some(format!("{}: {e:#}", t.error)),
            (Op::Move(Node::Part(part), folder), Ok(_)) => {
                self.part_moved(ctx, part.clone(), folder.clone())
            }
            (Op::Delete(Node::Part(part)), Ok(_)) => self.part_gone(ctx, part),
            (Op::Delete(Node::File(path)), Ok(_)) => {
                self.tab.notice = Some(format!("{path}: {}", t.loose_removed));
                self.reload(ctx);
            }
            (_, Ok(_)) => self.reload(ctx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str) -> Node {
        Node::Part(name.into())
    }

    #[test]
    fn summary_counts_and_says_stopped() {
        let report = Report {
            results: vec![
                (Op::Move(part("а"), "Стена".into()), Ok(Some(part("а")))),
                (
                    Op::Move(part("б"), "Стена".into()),
                    Err(anyhow::anyhow!("занято")),
                ),
            ],
            total: 5,
            stopped: true,
        };
        assert_eq!(report.summary(), "Перенесено 1 из 5, остановлено");
        assert_eq!(report.errors().as_deref(), Some("«б»: занято"));

        let deleted = Report {
            results: vec![(Op::Delete(Node::File("Стена/ТЗ.pdf".into())), Ok(None))],
            total: 1,
            stopped: false,
        };
        assert_eq!(deleted.summary(), "Удалено 1 из 1");
        assert_eq!(deleted.errors(), None);
    }
}
