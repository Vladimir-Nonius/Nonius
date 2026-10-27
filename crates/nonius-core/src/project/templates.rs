//! Файлы, которые `nonius init` кладёт в новый проект: `.gitattributes` и `AGENTS.md`.

use super::LFS_EXTENSIONS;

/// Шаблон без учёта регистра: `step` → `[sS][tT][eE][pP]`.
fn case_insensitive(ext: &str) -> String {
    ext.chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                format!("[{}{}]", c.to_ascii_lowercase(), c.to_ascii_uppercase())
            } else {
                c.to_string()
            }
        })
        .collect()
}

pub(super) fn gitattributes() -> String {
    let mut s = String::from(
        "# Создано Nonius.\n\
         # Текстовые файлы проекта — с переводом строки LF на любой ОС.\n\
         *.yaml text eol=lf\n\
         *.md text eol=lf\n\
         # Бинарные файлы CAD хранятся через Git LFS.\n",
    );
    for ext in LFS_EXTENSIONS {
        s.push_str(&format!(
            "*.{} filter=lfs diff=lfs merge=lfs -text\n",
            case_insensitive(ext)
        ));
    }
    s
}

pub(super) fn agents_md(name: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitattributes_cover_both_cases() {
        let g = gitattributes();
        assert!(g.contains("*.[sS][tT][eE][pP] filter=lfs"));
        assert!(g.contains("*.[xX]_[tT] filter=lfs"));
    }
}
