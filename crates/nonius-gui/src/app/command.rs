//! Одна команда на всё: клавиша, пункт меню и перетаскивание просят одно и
//! то же одними словами, а что делать — решается здесь. Несколько
//! предметов идут очередью (`batch.rs`).

use eframe::egui;

use super::batch::Op;
use super::node::Node;
use super::part_dialogs::{Confirm, Dialog};
use super::{structure, App};
use crate::texts::texts;

/// Что попросили сделать с предметами.
pub(super) enum Command {
    /// Удалить, с вопросом.
    Delete(Vec<Node>),
    /// Перенести в папку (пустая строка — корень).
    Move(Vec<Node>, String),
    /// Выделить всё в виде, где щёлкнули последним (Ctrl+A).
    SelectAll,
    /// Снять выделение (Esc).
    Unselect,
    /// Переименовать выделенную деталь (F2).
    Rename,
    /// Открыть выделенное (Enter): деталь — в 3D, файл — его программой.
    Open,
}

impl App {
    /// Исполнить команду.
    pub(super) fn command(&mut self, ctx: &egui::Context, command: Command) {
        match command {
            Command::Delete(nodes) => self.delete(nodes),
            Command::Move(nodes, folder) => {
                // Что уже лежит в этой папке, переносить некуда.
                let ops = nodes
                    .into_iter()
                    .filter(|n| match n {
                        Node::Part(name) => self.folder_of(name) != folder,
                        _ => true,
                    })
                    .map(|n| Op::Move(n, folder.clone()))
                    .collect();
                self.run_batch(ctx, ops);
            }
            Command::SelectAll => {
                let structure = self.tab.view == structure::View::Structure;
                let order = if self.tab.in_structure && structure {
                    &self.tab.structure_order
                } else {
                    &self.tab.tree_order
                };
                self.tab.selection.all(order);
            }
            Command::Unselect => self.tab.selection.clear(),
            Command::Rename => {
                if let Some(Node::Part(name)) = self.tab.selection.single().cloned() {
                    self.ask_rename(&name);
                }
            }
            Command::Open => match self.tab.selection.single().cloned() {
                Some(Node::Part(name)) if self.tab.part.as_ref() != Some(&name) => {
                    self.select_part(ctx, &name)
                }
                Some(Node::File(path)) => self.open_loose(ctx, path, false),
                _ => {}
            },
        }
    }

    /// Удалить: один предмет — прежним окошком, несколько — одним
    /// окошком со списком.
    fn delete(&mut self, nodes: Vec<Node>) {
        let t = texts();
        if nodes.iter().any(|n| matches!(n, Node::Folder(_))) {
            self.tab.notice = Some(t.cant_delete_folder.to_string());
            return;
        }
        match nodes.as_slice() {
            [] => {}
            [Node::Part(name)] => self.ask_delete(name),
            [Node::File(path)] => self.ask_remove_file("", path.clone()),
            _ => self.tab.dialog = Some(Dialog::DeleteMany(nodes, Confirm::default())),
        }
    }

    /// Открыто окошко: клавиши — его, а не выделенного.
    fn modal_open(&self) -> bool {
        let tab = &self.tab;
        tab.dialog.is_some()
            || tab.save.is_some()
            || tab.add.is_some()
            || tab.new_part.is_some()
            || tab.new_folder.is_some()
            || tab.settings.is_some()
            || tab.ask_upgrade
            || self.first_run.is_some()
            || self.closing.is_some()
    }
}

/// Клавиши выделенного: Delete, Ctrl+A, Esc, F2, Enter. Зовётся в конце
/// кадра, когда оба вида уже отметили свой порядок. Пока печатают в поле,
/// открыто окошко или меню — клавиши не наши.
pub(super) fn keys(app: &mut App, ctx: &egui::Context) {
    if app.tab.project.is_none()
        || app.modal_open()
        || ctx.wants_keyboard_input()
        || egui::Popup::is_any_open(ctx)
    {
        return;
    }
    let none = egui::Modifiers::NONE;
    let pressed = |mods, key| ctx.input_mut(|i| i.consume_key(mods, key));
    // Последним выбрали файл на вкладке «Файлы» — Delete про него, это
    // решает она.
    let command = if !app.tab.keys_to_files && pressed(none, egui::Key::Delete) {
        Some(Command::Delete(app.tab.selection.nodes()))
    } else if pressed(egui::Modifiers::COMMAND, egui::Key::A) {
        Some(Command::SelectAll)
    } else if pressed(none, egui::Key::Escape) {
        Some(Command::Unselect)
    } else if pressed(none, egui::Key::F2) {
        Some(Command::Rename)
    } else if pressed(none, egui::Key::Enter) {
        Some(Command::Open)
    } else {
        None
    };
    if let Some(command) = command {
        app.command(ctx, command);
    }
}
