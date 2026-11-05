//! Выделение: что человек отметил в дереве и «Структуре». Одно на оба вида:
//! отмеченное в одном подсвечено и в другом. Щелчок выделяет одно, Ctrl —
//! добавляет или убирает, Shift — всё от прошлого щелчка до этого. Открытая
//! в 3D деталь — отдельно (`tab.part`): выделить можно и не открывая.

use std::collections::BTreeSet;

use eframe::egui;

use super::node::Node;
use super::App;
use crate::texts::texts;

/// Отмеченные предметы и тот, от которого считает Shift.
#[derive(Default)]
pub(super) struct Selection {
    items: BTreeSet<Node>,
    anchor: Option<Node>,
    /// Тянут рамку: что было выделено до неё (с Ctrl) или пусто.
    before: Option<BTreeSet<Node>>,
}

/// Что значил щелчок.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Click {
    /// Обычный: деталь открыть, папку свернуть или раскрыть.
    Plain,
    /// С Ctrl или Shift: поменялось только выделение.
    Changed,
}

/// Корень проекта не выделяется: удалять и переносить его некуда.
fn is_root(node: &Node) -> bool {
    matches!(node, Node::Folder(path) if path.is_empty())
}

impl Selection {
    /// Выделен только этот предмет.
    pub fn only(&mut self, node: Node) {
        self.clear();
        if is_root(&node) {
            return;
        }
        self.items.insert(node.clone());
        self.anchor = Some(node);
    }

    /// Добавить предмет или убрать, если уже выделен.
    pub fn toggle(&mut self, node: Node) {
        if is_root(&node) {
            return;
        }
        if !self.items.remove(&node) {
            self.items.insert(node.clone());
        }
        self.anchor = Some(node);
    }

    /// Всё от прошлого щелчка до этого предмета в порядке `order` — как
    /// они стоят на экране. Прошлого щелчка нет в этом виде — только этот.
    pub fn range(&mut self, node: Node, order: &[Node]) {
        let from = self
            .anchor
            .as_ref()
            .and_then(|a| order.iter().position(|n| n == a));
        let to = order.iter().position(|n| *n == node);
        let (Some(from), Some(to)) = (from, to) else {
            self.only(node);
            return;
        };
        let (a, b) = (from.min(to), from.max(to));
        self.items = order[a..=b]
            .iter()
            .filter(|n| !is_root(n))
            .cloned()
            .collect();
    }

    /// Тянут рамку: выделено то, что она задела, а с Ctrl (`add`) — ещё и
    /// то, что было до неё. Зовётся каждый кадр, пока тянут.
    pub fn sweep(&mut self, touched: Vec<Node>, add: bool) {
        let items = &self.items;
        let before =
            self.before
                .get_or_insert_with(|| if add { items.clone() } else { BTreeSet::new() });
        let mut now = before.clone();
        now.extend(touched.iter().filter(|n| !is_root(n)).cloned());
        self.items = now;
        if let Some(first) = touched.into_iter().find(|n| !is_root(n)) {
            self.anchor = Some(first);
        }
    }

    /// Рамку отпустили.
    pub fn swept(&mut self) {
        self.before = None;
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.anchor = None;
    }

    pub fn contains(&self, node: &Node) -> bool {
        self.items.contains(node)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Единственный выделенный предмет.
    pub fn single(&self) -> Option<&Node> {
        match self.items.len() {
            1 => self.items.iter().next(),
            _ => None,
        }
    }

    /// Оставить только то, что ещё есть в проекте.
    pub fn retain(&mut self, keep: impl Fn(&Node) -> bool) {
        self.items.retain(|n| keep(n));
        if self.anchor.as_ref().is_some_and(|a| !keep(a)) {
            self.anchor = None;
        }
    }

    /// Щелчок по предмету: правила для всех видов в одном месте.
    pub fn click(&mut self, node: Node, mods: egui::Modifiers, order: &[Node]) -> Click {
        if is_root(&node) {
            self.clear();
            return Click::Plain;
        }
        if mods.shift {
            self.range(node, order);
            Click::Changed
        } else if mods.command {
            self.toggle(node);
            Click::Changed
        } else {
            self.only(node);
            Click::Plain
        }
    }
}

/// Сказать диктору экрана (и тестам), выделена ли строка: кнопка egui
/// рисует выделение, но вслух о нём не говорит.
pub(super) fn tell(r: &egui::Response, chosen: bool, text: &str) {
    r.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, chosen, text)
    });
}

/// Скругление строки и карточки: у рамки открытой детали те же углы.
pub(super) const CORNER: f32 = 4.0;

/// Открытая в 3D, но не выделенная деталь: заметная рамка цветом
/// выделения, внутри — обычный фон.
pub(super) fn opened_frame(ui: &egui::Ui, rect: egui::Rect) {
    let stroke = egui::Stroke::new(2.0_f32, ui.visuals().selection.bg_fill);
    ui.painter()
        .rect_stroke(rect, CORNER, stroke, egui::StrokeKind::Inside);
}

impl App {
    /// Щёлкнули по предмету в дереве (`tree`) или «Структуре». Правая
    /// кнопка по выделенному выделение не трогает: меню — про него.
    pub(super) fn pick(
        &mut self,
        ctx: &egui::Context,
        node: Node,
        secondary: bool,
        order: &[Node],
        tree: bool,
    ) {
        self.tab.keys_to_files = false;
        if secondary && self.tab.selection.contains(&node) {
            return;
        }
        // Правая кнопка с Ctrl — всё равно обычный щелчок: меню про один.
        let mods = if secondary {
            egui::Modifiers::default()
        } else {
            ctx.input(|i| i.modifiers)
        };
        if self.tab.selection.click(node.clone(), mods, order) == Click::Changed {
            return;
        }
        match node {
            Node::Part(name) => {
                // Уже открытую не открывать заново: сбросилось бы сравнение.
                if self.tab.part.as_ref() != Some(&name) {
                    self.select_part(ctx, &name);
                } else if !secondary {
                    self.tab.expanded.insert(name.clone());
                }
                if let Some(p) = self.tab.parts.iter().find(|p| p.name == name) {
                    self.tab.place = p.folder.clone();
                }
            }
            Node::Folder(path) if tree => {
                // Сворачивать — только левой кнопкой: правая открывает меню.
                if !secondary && !path.is_empty() && !self.tab.folded.remove(&path) {
                    self.tab.folded.insert(path.clone());
                }
                self.tab.place = path;
            }
            Node::Folder(_) | Node::File(_) => {}
        }
    }

    /// Delete в дереве или «Структуре»: удалить выделенное, с вопросом.
    pub(super) fn delete_selected(&mut self) {
        let t = texts();
        match self.tab.selection.single().cloned() {
            Some(Node::Part(name)) => self.ask_delete(&name),
            Some(Node::File(path)) => self.ask_remove_file("", path),
            Some(Node::Folder(_)) => self.tab.notice = Some(t.cant_delete_folder.to_string()),
            None if self.tab.selection.is_empty() => {}
            None => self.tab.notice = Some(t.cant_delete_many.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str) -> Node {
        Node::Part(name.into())
    }

    fn order() -> Vec<Node> {
        vec![
            Node::Folder("Стена".into()),
            part("A"),
            part("B"),
            Node::File("Стена/лист.dxf".into()),
            part("C"),
        ]
    }

    fn items(s: &Selection) -> Vec<Node> {
        s.items.iter().cloned().collect()
    }

    #[test]
    fn only_one() {
        let mut s = Selection::default();
        s.only(part("A"));
        s.only(part("B"));
        assert_eq!(items(&s), [part("B")]);
        assert_eq!(s.single(), Some(&part("B")));
    }

    #[test]
    fn toggle_there_and_back() {
        let mut s = Selection::default();
        s.toggle(part("A"));
        s.toggle(part("B"));
        assert_eq!(s.items.len(), 2);
        assert_eq!(s.single(), None);
        s.toggle(part("A"));
        assert_eq!(items(&s), [part("B")]);
    }

    #[test]
    fn range_both_ways() {
        let order = order();
        let mut s = Selection::default();
        s.only(part("A"));
        s.range(part("C"), &order);
        assert_eq!(s.items.len(), 4);
        assert!(s.contains(&Node::File("Стена/лист.dxf".into())));
        // Shift не двигает начало: следующий отрезок — снова от A.
        s.range(Node::Folder("Стена".into()), &order);
        assert_eq!(items(&s), [part("A"), Node::Folder("Стена".into())]);
    }

    #[test]
    fn range_without_anchor_is_one() {
        let order = order();
        let mut s = Selection::default();
        s.range(part("B"), &order);
        assert_eq!(items(&s), [part("B")]);
        // Начало в другом виде, здесь его нет — тоже только этот.
        s.only(part("X"));
        s.range(part("C"), &order);
        assert_eq!(items(&s), [part("C")]);
    }

    #[test]
    fn retain_forgets_the_anchor() {
        let order = order();
        let mut s = Selection::default();
        s.only(part("A"));
        s.retain(|n| *n != part("A"));
        assert!(s.is_empty());
        s.range(part("C"), &order);
        assert_eq!(items(&s), [part("C")]);
    }

    #[test]
    fn root_is_never_selected() {
        let root = Node::Folder(String::new());
        let mut s = Selection::default();
        s.only(part("A"));
        assert_eq!(
            s.click(root.clone(), egui::Modifiers::COMMAND, &[]),
            Click::Plain
        );
        assert!(s.is_empty());
        s.toggle(root.clone());
        s.only(root);
        assert!(s.is_empty());
    }

    #[test]
    fn sweep_replaces_or_adds() {
        let mut s = Selection::default();
        s.only(part("A"));
        // Рамка растёт и сжимается: выделено только то, что задето сейчас.
        s.sweep(vec![part("B"), part("C")], false);
        s.sweep(vec![part("B")], false);
        assert_eq!(items(&s), [part("B")]);
        s.swept();
        // С Ctrl — к прежнему.
        s.sweep(vec![part("C")], true);
        s.sweep(vec![part("C"), Node::Folder(String::new())], true);
        assert_eq!(items(&s), [part("B"), part("C")]);
        s.swept();
        s.sweep(Vec::new(), false);
        assert!(s.is_empty());
    }

    #[test]
    fn modifiers_decide() {
        let order = order();
        let mut s = Selection::default();
        assert_eq!(
            s.click(part("A"), egui::Modifiers::NONE, &order),
            Click::Plain
        );
        assert_eq!(
            s.click(part("B"), egui::Modifiers::COMMAND, &order),
            Click::Changed
        );
        assert_eq!(s.items.len(), 2);
        assert_eq!(
            s.click(part("C"), egui::Modifiers::SHIFT, &order),
            Click::Changed
        );
        // От B (последний Ctrl) до C.
        assert_eq!(s.items.len(), 3);
    }
}
