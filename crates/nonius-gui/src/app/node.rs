//! Что это на экране: деталь, папка или обычный файл папки. Дерево,
//! «Структура», бросок из Проводника и перетаскивание называют одно и то же
//! одним словом, а не каждый своей строкой.

use super::tree_folders::parent;

/// Предмет проекта, который видно в окне.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Node {
    /// Деталь: её имя.
    Part(String),
    /// Папка-группа: путь от корня через `/`, пустой — корень.
    Folder(String),
    /// Обычный файл папки: путь от корня через `/`.
    File(String),
}

impl Node {
    /// Куда ляжет брошенное на этот предмет: на файл — в его папку.
    pub fn landing(&self) -> Node {
        match self {
            Node::File(path) => Node::Folder(parent(path).to_string()),
            other => other.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_lands_in_its_folder() {
        let file = Node::File("Стена/чертёж.pdf".into());
        assert_eq!(file.landing(), Node::Folder("Стена".into()));
        assert_eq!(
            Node::File("заметка.txt".into()).landing(),
            Node::Folder(String::new())
        );
        let part = Node::Part("RAIL-01".into());
        assert_eq!(part.landing(), part);
    }
}
