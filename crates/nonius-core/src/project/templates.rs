//! Файлы, которые `nonius init` кладёт в новый проект: `.gitattributes`.

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
