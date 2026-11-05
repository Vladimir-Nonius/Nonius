//! Что где на экране в этом кадре. Строки дерева, блоки и карточки
//! «Структуры» отмечают свой прямоугольник, и по этим отметкам видно, на
//! что показывает курсор: куда бросили файл из Проводника, куда несут
//! деталь. Отметки живут один кадр — перед рисованием и при переключении
//! вкладки список чистится.

use std::collections::HashSet;

use eframe::egui;

use super::node::Node;

/// Предмет и его место на экране.
#[derive(Debug, Clone)]
pub(super) struct Shown {
    pub rect: egui::Rect,
    pub node: Node,
}

impl Shown {
    pub fn new(rect: egui::Rect, node: Node) -> Self {
        Self { rect, node }
    }

    /// Только видимая часть: что ушло за прокрутку, не ловит курсор и не
    /// подсвечивается поверх соседних панелей.
    pub fn on(ui: &egui::Ui, rect: egui::Rect, node: Node) -> Self {
        Self::new(rect.intersect(ui.clip_rect()), node)
    }
}

/// Предмет под точкой. Место целиком — вся папка, внутри неё деталь или
/// вложенная папка: главнее самое мелкое.
pub(super) fn under(list: &[Shown], pos: egui::Pos2) -> Option<&Shown> {
    list.iter()
        .filter(|s| s.rect.contains(pos))
        .min_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
}

/// Всё место предмета под точкой: у папки — блок или строка вместе с
/// содержимым, а не только заголовок.
pub(super) fn whole(list: &[Shown], node: &Node, pos: egui::Pos2) -> Option<egui::Rect> {
    list.iter()
        .filter(|s| &s.node == node && s.rect.contains(pos))
        .map(|s| s.rect)
        .max_by(|a, b| a.area().total_cmp(&b.area()))
}

/// Предметы в том порядке, как они стоят в виде: по нему Shift выделяет
/// отрезок. Папка — по своей строке или заголовку, раньше содержимого;
/// корень не выделяется. Не по месту на экране: что ушло за прокрутку,
/// обрезано в пустое, а в порядке нужно и оно.
pub(super) fn order(list: &[Shown]) -> Vec<Node> {
    let mut seen = HashSet::new();
    list.iter()
        .map(|s| &s.node)
        .filter(|n| !matches!(n, Node::Folder(path) if path.is_empty()))
        .filter(|n| seen.insert(*n))
        .cloned()
        .collect()
}

/// Что задела рамка мышью, в порядке вида. У предмета берётся самое
/// мелкое его место: у папки — строка или заголовок, а не всё, что в ней.
/// `rows` — строки списка: задета, если рамка прошла по её высоте.
pub(super) fn touched(list: &[Shown], area: egui::Rect, rows: bool) -> Vec<Node> {
    let hit = |r: egui::Rect| {
        if rows {
            r.y_range().intersects(area.y_range())
        } else {
            r.intersects(area)
        }
    };
    order(list)
        .into_iter()
        .filter(|node| {
            list.iter()
                .filter(|s| &s.node == node && s.rect.is_positive())
                .min_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
                .is_some_and(|s| hit(s.rect))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_touches_rows_and_headers() {
        let wall = Node::Folder("Стена".into());
        let a = Node::Part("A".into());
        let b = Node::Part("B".into());
        let list = [
            at(0.0, 0.0, 100.0, 20.0, wall.clone()),
            at(10.0, 20.0, 60.0, 20.0, a.clone()),
            at(0.0, 0.0, 200.0, 80.0, wall.clone()),
            at(10.0, 60.0, 60.0, 20.0, b.clone()),
        ];
        // Справа от коротких строк: в дереве задеты по высоте.
        let right = egui::Rect::from_min_max(egui::pos2(150.0, 25.0), egui::pos2(190.0, 70.0));
        assert_eq!(touched(&list, right, true), [a.clone(), b]);
        // В «Структуре» — только то, что рамка накрыла; папка — по
        // заголовку, не по всему блоку.
        assert!(touched(&list, right, false).is_empty());
        let card = egui::Rect::from_min_max(egui::pos2(20.0, 25.0), egui::pos2(30.0, 30.0));
        assert_eq!(touched(&list, card, false), [a]);
    }

    #[test]
    fn order_as_shown() {
        let wall = Node::Folder("Стена".into());
        let a = Node::Part("A".into());
        let list = [
            at(0.0, 0.0, 0.0, 0.0, wall.clone()),
            at(0.0, 20.0, 100.0, 20.0, a.clone()),
            at(0.0, 0.0, 100.0, 40.0, wall.clone()),
            at(0.0, 0.0, 100.0, 300.0, Node::Folder(String::new())),
        ];
        assert_eq!(order(&list), [wall, a]);
    }

    fn at(x: f32, y: f32, w: f32, h: f32, node: Node) -> Shown {
        Shown::new(
            egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h)),
            node,
        )
    }

    #[test]
    fn smallest_wins() {
        let list = [
            at(0.0, 0.0, 400.0, 300.0, Node::Folder("Стена".into())),
            at(10.0, 40.0, 130.0, 50.0, Node::Part("RAIL-01".into())),
        ];
        let card = under(&list, egui::pos2(20.0, 50.0)).map(|s| &s.node);
        assert_eq!(card, Some(&Node::Part("RAIL-01".into())));
        let folder = under(&list, egui::pos2(300.0, 200.0)).map(|s| &s.node);
        assert_eq!(folder, Some(&Node::Folder("Стена".into())));
        assert!(under(&list, egui::pos2(500.0, 10.0)).is_none());
    }

    #[test]
    fn whole_folder_not_its_header() {
        let wall = Node::Folder("Стена".into());
        let list = [
            at(0.0, 0.0, 400.0, 30.0, wall.clone()),
            at(0.0, 0.0, 400.0, 300.0, wall.clone()),
        ];
        let rect = whole(&list, &wall, egui::pos2(10.0, 10.0)).unwrap();
        assert_eq!(rect.height(), 300.0);
    }
}
