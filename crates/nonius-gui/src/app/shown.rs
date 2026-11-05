//! Что где на экране в этом кадре. Строки дерева, блоки и карточки
//! «Структуры» отмечают свой прямоугольник, и по этим отметкам видно, на
//! что показывает курсор: куда бросили файл из Проводника, куда несут
//! деталь. Отметки живут один кадр — перед рисованием и при переключении
//! вкладки список чистится.

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
}

/// Предмет под точкой. Место целиком — вся папка, внутри неё деталь или
/// вложенная папка: главнее самое мелкое.
pub(super) fn under(list: &[Shown], pos: egui::Pos2) -> Option<&Shown> {
    list.iter()
        .filter(|s| s.rect.contains(pos))
        .min_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
