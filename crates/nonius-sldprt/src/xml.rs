//! Терпимое чтение XML из секций файла SolidWorks.
//!
//! Это не полный разбор XML, а ровно то, что нужно секциям `swXmlContents`
//! и `docProps`: элементы, атрибуты, текст. Читаем чужие файлы, поэтому на
//! мусоре не падаем: что разобралось — отдаём, а про остальное говорим
//! флажком [`Doc::clean`] (`false` — документ оборван, теги не сходятся
//! или в тексте были нечитаемые байты).
//!
//! Кодировки: UTF-8 (обычно), UTF-16 с меткой порядка байтов (так лежит
//! `MATERIALTREE`) и windows-1251, если она названа в заголовке `<?xml`.
//! Перед `<?xml` бывает служебный байт (`0x86` или `0x85` у `KeyWords`) —
//! пропускаем всё до первого `<`.
//!
//! Память и время — пропорционально размеру входа; глубина вложенности
//! ограничена [`MAX_DEPTH`].

/// Глубже элементы не вкладываем: настоящие секции не глубже десятка.
const MAX_DEPTH: usize = 64;
/// Сколько байт мусора терпим перед первым `<`.
const MAX_JUNK: usize = 8;

/// Элемент XML.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Node {
    /// Имя как в файле, с приставкой: `vt:lpstr`.
    pub name: String,
    /// Атрибуты в порядке файла.
    pub attrs: Vec<(String, String)>,
    /// Текст прямо внутри элемента.
    pub text: String,
    /// Вложенные элементы.
    pub children: Vec<Node>,
}

/// Разобранный документ.
#[derive(Debug, Clone, PartialEq)]
pub struct Doc {
    /// Корневой элемент.
    pub root: Node,
    /// Документ разобран без потерь.
    pub clean: bool,
}

impl Node {
    /// Имя без приставки пространства имён.
    pub fn local(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }

    /// Значение атрибута.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// Атрибут как целое число.
    pub fn number(&self, name: &str) -> Option<i64> {
        self.attr(name)?.trim().parse().ok()
    }

    /// Атрибут-флажок `YES` / `NO` (или `1` / `0`).
    pub fn flag(&self, name: &str) -> Option<bool> {
        match self.attr(name)?.trim() {
            v if v.eq_ignore_ascii_case("yes") || v == "1" => Some(true),
            v if v.eq_ignore_ascii_case("no") || v == "0" => Some(false),
            _ => None,
        }
    }

    /// Прямые потомки с именем `local`.
    pub fn named<'a>(&'a self, local: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.local() == local)
    }

    /// Первый прямой потомок с именем `local`.
    pub fn child(&self, local: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.local() == local)
    }
}

/// Байты секции → текст. Второе значение — без потерь ли.
fn decode(raw: &[u8]) -> Option<(String, bool)> {
    let utf16 = |be: bool| {
        let units: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|p| {
                if be {
                    u16::from_be_bytes([p[0], p[1]])
                } else {
                    u16::from_le_bytes([p[0], p[1]])
                }
            })
            .collect();
        let text = String::from_utf16_lossy(&units);
        let clean = !text.contains('\u{fffd}') && raw.len() % 2 == 0;
        (text, clean)
    };
    if raw.starts_with(&[0xff, 0xfe]) {
        return Some(utf16(false));
    }
    if raw.starts_with(&[0xfe, 0xff]) {
        return Some(utf16(true));
    }
    let start = raw.iter().take(MAX_JUNK).position(|b| *b == b'<')?;
    let raw = &raw[start..];
    let head = &raw[..raw.len().min(120)];
    let named = |code: &[u8]| {
        head.windows(code.len())
            .any(|w| w.eq_ignore_ascii_case(code))
    };
    if named(b"windows-1251") || named(b"cp1251") {
        return Some((raw.iter().map(|b| cp1251(*b)).collect(), true));
    }
    let text = String::from_utf8_lossy(raw);
    let clean = !text.contains('\u{fffd}');
    Some((text.into_owned(), clean))
}

/// Байт windows-1251 → буква. Русские буквы — точно, редкие знаки — `?`.
fn cp1251(byte: u8) -> char {
    match byte {
        0x00..=0x7f => char::from(byte),
        0xa8 => 'Ё',
        0xb8 => 'ё',
        0xb0 => '°',
        0xa0 => '\u{a0}',
        0xab => '«',
        0xbb => '»',
        0xb9 => '№',
        0xc0..=0xff => char::from_u32(0x410 + u32::from(byte - 0xc0)).unwrap_or('?'),
        _ => '?',
    }
}

/// Раскрыть `&lt;`, `&amp;`, `&#1076;` и прочие. Незнакомое оставить.
fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let known = rest.find(';').filter(|end| *end <= 10).and_then(|end| {
            let found = match &rest[1..end] {
                "lt" => Some('<'),
                "gt" => Some('>'),
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                code => {
                    let number = code.strip_prefix('#')?;
                    let value = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => number.parse().ok()?,
                    };
                    char::from_u32(value)
                }
            };
            found.map(|c| (c, end + 1))
        });
        match known {
            Some((c, skip)) => {
                out.push(c);
                rest = &rest[skip..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Конец тега, начатого в `text[0] == '<'`: место `>` вне кавычек.
fn tag_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (at, c) in text.char_indices() {
        match (quote, c) {
            (None, '>') => return Some(at),
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if q == c => quote = None,
            _ => {}
        }
    }
    None
}

/// Открывающий тег без скобок (`name a="1" b='2'`) → элемент.
fn element(inside: &str) -> Node {
    let inside = inside.trim();
    let split = inside.find(char::is_whitespace).unwrap_or(inside.len());
    let mut node = Node {
        name: inside[..split].to_string(),
        ..Node::default()
    };
    let mut rest = inside[split..].trim_start();
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].trim();
        let value = rest[eq + 1..].trim_start();
        let Some(quote) = value.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
            break;
        };
        let Some(end) = value[1..].find(quote) else {
            break;
        };
        node.attrs
            .push((name.to_string(), unescape(&value[1..=end])));
        rest = value[end + 2..].trim_start();
    }
    node
}

/// Разобрать секцию. `None` — это вообще не XML.
pub fn parse(raw: &[u8]) -> Option<Doc> {
    let (text, mut clean) = decode(raw)?;
    // Внизу стопки — мнимый корень: к нему цепляются элементы верхнего уровня.
    let mut stack = vec![Node::default()];
    let close = |stack: &mut Vec<Node>| {
        if let Some(done) = stack.pop() {
            if let Some(parent) = stack.last_mut() {
                parent.children.push(done);
            }
        }
    };
    let mut rest = text.as_str();
    while let Some(at) = rest.find('<') {
        if let Some(top) = stack.last_mut() {
            top.text.push_str(&unescape(&rest[..at]));
        }
        rest = &rest[at..];
        let skip_to = |end: &str| rest.find(end).map(|p| p + end.len());
        if let Some(body) = rest.strip_prefix("<![CDATA[") {
            let end = body.find("]]>").unwrap_or(body.len());
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&body[..end]);
            }
            rest = body.get(end + 3..).unwrap_or("");
            continue;
        }
        let special = [("<?", "?>"), ("<!--", "-->"), ("<!", ">")];
        if let Some((_, end)) = special.iter().find(|(start, _)| rest.starts_with(start)) {
            match skip_to(end) {
                Some(next) => rest = &rest[next..],
                None => {
                    clean = false;
                    rest = "";
                }
            }
            continue;
        }
        let Some(end) = tag_end(rest) else {
            clean = false;
            rest = "";
            break;
        };
        let inside = &rest[1..end];
        rest = &rest[end + 1..];
        if let Some(name) = inside.strip_prefix('/') {
            let name = name.trim();
            match stack.iter().rposition(|n| n.name == name) {
                Some(open) if open > 0 => {
                    clean &= open + 1 == stack.len();
                    while stack.len() > open {
                        close(&mut stack);
                    }
                }
                _ => clean = false,
            }
        } else if let Some(empty) = inside.strip_suffix('/') {
            if let Some(top) = stack.last_mut() {
                top.children.push(element(empty));
            }
        } else if stack.len() > MAX_DEPTH {
            clean = false;
        } else {
            stack.push(element(inside));
        }
    }
    clean &= stack.len() == 1 && rest.trim().is_empty();
    while stack.len() > 1 {
        close(&mut stack);
    }
    let root = stack.pop()?.children.into_iter().next()?;
    Some(Doc { root, clean })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_elements_attributes_text_and_entities() {
        let doc = parse(
            "\u{86}<?xml version=\"1.0\"?><!-- c --><K id=\"7\" Name=\"a &lt;b&gt; &#1076;\">\
             <vt:lpstr>x &amp; y<![CDATA[<z>]]></vt:lpstr><E a='1'/></K>"
                .as_bytes(),
        );
        // Служебный байт перед `<?xml` в настоящем файле один; здесь он
        // записан как два байта UTF-8 — тоже пропускается.
        let doc = doc.unwrap();
        assert!(doc.clean);
        assert_eq!(doc.root.name, "K");
        assert_eq!(doc.root.number("id"), Some(7));
        assert_eq!(doc.root.attr("Name"), Some("a <b> д"));
        let text = doc.root.child("lpstr").unwrap();
        assert_eq!((text.local(), text.text.as_str()), ("lpstr", "x & y<z>"));
        assert_eq!(doc.root.child("E").unwrap().attr("a"), Some("1"));
        assert_eq!(doc.root.named("E").count(), 1);
    }

    #[test]
    fn other_encodings_are_read() {
        let utf16: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain(
                "<m name=\"сталь\"/>"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes),
            )
            .collect();
        assert_eq!(parse(&utf16).unwrap().root.attr("name"), Some("сталь"));
        let mut cp = b"<?xml version=\"1.0\" encoding=\"windows-1251\"?><m n=\"".to_vec();
        cp.extend([0xd1, 0xf2, 0xe0, 0xeb, 0xfc]); // «Сталь»
        cp.extend(b"\"/>");
        let doc = parse(&cp).unwrap();
        assert_eq!(doc.root.attr("n"), Some("Сталь"));
        assert!(doc.clean);
    }

    #[test]
    fn garbage_gives_what_was_read_and_says_so() {
        // Оборван посреди тега: первый потомок цел.
        let cut = parse(b"<K id=\"1\"><A n=\"1\"/><B n=\"2").unwrap();
        assert!(!cut.clean);
        assert_eq!(cut.root.children.len(), 1);
        // Теги не сходятся, в тексте битый байт.
        let mixed = parse(b"<K><A><B></A>\xff</K>").unwrap();
        assert!(!mixed.clean);
        assert_eq!(mixed.root.child("A").unwrap().children.len(), 1);
        // Лишний закрывающий тег и хвост после корня.
        assert!(!parse(b"<K></X></K>").unwrap().clean);
        assert!(!parse(b"<K/>tail").unwrap().clean);
        // Не XML вовсе.
        assert_eq!(parse(b"\x00\x01\x02\x03\x04\x05\x06\x07\x08<K/>"), None);
        assert_eq!(parse(b""), None);
        assert_eq!(parse(b"plain text"), None);
        // Глубина ограничена, память не растёт без меры.
        let deep = "<a>".repeat(10_000);
        assert!(!parse(deep.as_bytes()).unwrap().clean);
    }
}
