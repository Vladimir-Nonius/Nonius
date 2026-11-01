//! `AGENTS.md` проекта: что читает ИИ, когда приходит в проект. Правила Nonius
//! лежат между двумя метками, поэтому их можно обновить в старых проектах и не
//! тронуть текст, который написали люди.

use std::fs;

use super::Project;
use crate::error::{Error, Result};

/// Имя файла.
pub const AGENTS_FILE: &str = "AGENTS.md";

const START: &str =
    "<!-- nonius:начало. Этот блок обновляет `nonius agents`, свой текст пишите вне его -->";
const END: &str = "<!-- nonius:конец -->";

/// Правила Nonius с метками.
fn block() -> String {
    format!(
        "{START}\n\
Проект ведётся в Nonius — системе контроля версий для CAD.\n\n\
## С чего начать\n\
- `nonius overview` — весь проект одним ответом. `--changed` — только несохранённое.\n\
- `nonius describe` — все команды, параметры и коды ошибок.\n\
- Добавляй `--json`: ответ `ok` и `data` либо `error.code`. Смотри на код, а не на текст.\n\n\
## Как устроен проект\n\
- `parts/<имя>/` и `assemblies/<имя>/` — детали и сборки.\n\
- `meta.yaml` — масса, объём, габариты, материал, свойства.\n\
- `tree.yaml` — дерево построения, если модель из SolidWorks.\n\
- `docs/` — чертежи, PDF, DXF.\n\n\
## Частые команды\n\
- `nonius status [деталь]` — что не сохранено.\n\
- `nonius log <деталь>` — версии.\n\
- `nonius diff <деталь> [v1] [v2]` — что изменилось, без чтения моделей.\n\
- `nonius files <деталь> [v1] [v2] [--text файл]` — файлы и разница текста.\n\
- `nonius material <деталь> [материал]` — паспорт и масса.\n\
- `nonius commit <деталь> -m \"что и почему\"` — новая версия.\n\n\
## Правила для ИИ\n\
- Не читай файлы моделей: `model.*`, `*.step`, `*.sldprt`, `*.sldasm`. Они огромные и бесполезны как текст. Числа есть в `meta.yaml` и `nonius diff`.\n\
- Подписывай свои действия: `--ai <имя> --ai-model <модель>` или переменные `NONIUS_AI` и `NONIUS_AI_MODEL`.\n\
- Версию фиксируй и откатывай только по просьбе человека. Решает человек.\n\
- Не удаляй историю и не снимай чужие блокировки.\n\
- Пиши, почему сделано изменение, а не только что изменено.\n\
{END}\n"
    )
}

/// `AGENTS.md` нового проекта.
pub(super) fn agents_md(name: &str) -> String {
    format!("# {name}\n\n{}", block())
}

/// Файл, который писали прежние версии Nonius: его можно заменить целиком.
fn first_template(name: &str) -> String {
    format!(
        "# {name}\n\n\
Проект ведётся в Nonius — системе контроля версий для CAD.\n\n\
## Как устроен проект\n\
- `parts/<имя>/` и `assemblies/<имя>/` — детали и сборки.\n\
  - `meta.yaml` — масса, объём, габариты, материал, свойства.\n\
  - `tree.yaml` — дерево построения (если модель из SolidWorks).\n\
  - `model.step` — геометрия, `docs/` — чертежи, PDF, DXF.\n\
- `notes/` — заметки. Ссылки: `[[деталь]]`, `[[деталь@v3]]`, `[[деталь#Фича]]`.\n\n\
## Правила для ИИ\n\
- Читай `meta.yaml`, `tree.yaml` и заметки — модели открывать не нужно.\n\
- Изменения предлагай как вариант; в основную линию — только если лидер разрешил.\n\
- Не удаляй историю и не снимай чужие блокировки.\n\
- Пиши, почему сделано изменение, а не только что изменено.\n"
    )
}

/// Новый текст файла; `None` — обновлять нечего. Текст людей не теряется:
/// заменяется только блок между метками или нетронутый старый шаблон.
fn updated(text: &str, name: &str) -> Option<String> {
    let block = block();
    let plain = text.replace("\r\n", "\n");
    if plain.contains(&block) {
        return None;
    }
    if plain == first_template(name) {
        return Some(agents_md(name));
    }
    if let (Some(start), Some(end)) = (plain.find(START), plain.find(END)) {
        if start < end {
            let after = &plain[end + END.len()..];
            let after = after.strip_prefix('\n').unwrap_or(after);
            return Some(format!("{}{block}{after}", &plain[..start]));
        }
    }
    let plain = plain.trim_end();
    Some(format!("{plain}\n\n{block}"))
}

impl Project {
    /// Правила для ИИ в `AGENTS.md` устарели или их нет.
    pub fn agents_outdated(&self) -> bool {
        match fs::read_to_string(self.root.join(AGENTS_FILE)) {
            Ok(text) => updated(&text, &self.config.name).is_some(),
            Err(_) => true,
        }
    }

    /// Обновить правила для ИИ. Возвращает `true`, если файл изменился.
    pub fn update_agents(&self) -> Result<bool> {
        let path = self.root.join(AGENTS_FILE);
        let name = &self.config.name;
        let text = if path.exists() {
            let old = fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
            match updated(&old, name) {
                Some(text) => text,
                None => return Ok(false),
            }
        } else {
            agents_md(name)
        };
        fs::write(&path, text).map_err(|e| Error::io(&path, e))?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_file_is_current() {
        assert_eq!(updated(&agents_md("Станок"), "Станок"), None);
    }

    #[test]
    fn untouched_old_template_is_replaced() {
        let old = first_template("Станок");
        assert_eq!(updated(&old, "Станок"), Some(agents_md("Станок")));
        let crlf = old.replace('\n', "\r\n");
        assert_eq!(updated(&crlf, "Станок"), Some(agents_md("Станок")));
    }

    #[test]
    fn edited_file_keeps_people_text() {
        let old = first_template("Станок") + "\nНаш допуск 0,1 мм.\n";
        let new = updated(&old, "Станок").unwrap();
        assert!(new.starts_with(&old.trim_end().to_string()));
        assert!(new.ends_with(&block()));
        assert_eq!(updated(&new, "Станок"), None);
    }

    #[test]
    fn block_is_replaced_between_marks() {
        let old = format!("До\n\n{START}\nстарое\n{END}\n\nПосле\n");
        let new = updated(&old, "Станок").unwrap();
        assert_eq!(new, format!("До\n\n{}\nПосле\n", block()));
    }

    #[test]
    fn broken_marks_lose_nothing() {
        let old = format!("{END}\nважно\n{START}\n");
        let new = updated(&old, "Станок").unwrap();
        assert!(new.starts_with(old.trim_end()));
        assert!(new.ends_with(&block()));
    }
}
