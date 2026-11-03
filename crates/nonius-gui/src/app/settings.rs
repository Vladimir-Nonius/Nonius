//! Что окно помнит между запусками: открытые вкладки проектов и переключатели
//! вида.
//!
//! Файл `window.txt` в папке настроек пользователя (Windows — `%APPDATA%\Nonius`),
//! строки `ключ=значение`. Испорченный или чужой файл не мешает запуску:
//! непонятные строки пропускаются.

use std::path::PathBuf;

/// Настройки окна.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Папки открытых вкладок по порядку.
    pub tabs: Vec<PathBuf>,
    /// Какая вкладка была открыта.
    pub active: usize,
    /// Серую деталь прозрачной.
    pub xray: bool,
    /// Рисовать рёбра.
    pub edges: bool,
    /// Колесо мыши наоборот.
    pub wheel_reverse: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            xray: true,
            edges: true,
            wheel_reverse: false,
        }
    }
}

impl Settings {
    /// Разобрать текст файла.
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let flag = value.trim() == "1";
            match key.trim() {
                "tab" if !value.trim().is_empty() => s.tabs.push(PathBuf::from(value.trim())),
                "active" => s.active = value.trim().parse().unwrap_or(0),
                // Так писала v0.3: один последний проект.
                "last_project" if !value.trim().is_empty() && s.tabs.is_empty() => {
                    s.tabs.push(PathBuf::from(value.trim()))
                }
                "xray" => s.xray = flag,
                "edges" => s.edges = flag,
                "wheel_reverse" => s.wheel_reverse = flag,
                _ => {}
            }
        }
        s
    }

    /// Текст файла.
    pub fn render(&self) -> String {
        let flag = |b: bool| if b { "1" } else { "0" };
        let tabs: String = self
            .tabs
            .iter()
            .map(|p| format!("tab={}\n", p.to_string_lossy()))
            .collect();
        format!(
            "{tabs}active={}\nxray={}\nedges={}\nwheel_reverse={}\n",
            self.active,
            flag(self.xray),
            flag(self.edges),
            flag(self.wheel_reverse),
        )
    }

    /// Прочитать из папки настроек пользователя.
    pub fn load() -> Self {
        file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    /// Записать. Ошибки записи не мешают работе — настройки просто не запомнятся.
    pub fn save(&self) {
        if let Some(f) = file() {
            if let Some(dir) = f.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(f, self.render());
        }
    }
}

/// Папка черновиков: `%LOCALAPPDATA%/Nonius/drafts` — не в перемещаемом
/// профиле, проекты бывают большими.
pub fn drafts_dir() -> Option<PathBuf> {
    let env = |k| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let dir = env("NONIUS_CONFIG_DIR")
        .or_else(|| env("LOCALAPPDATA").map(|d| d.join("Nonius")))
        .or_else(|| file().and_then(|f| f.parent().map(PathBuf::from)))?;
    Some(dir.join("drafts"))
}

/// Путь к файлу настроек. `NONIUS_CONFIG_DIR` — для тестов и переносной установки.
fn file() -> Option<PathBuf> {
    let env = |k| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let dir = env("NONIUS_CONFIG_DIR")
        .or_else(|| env("APPDATA").map(|d| d.join("Nonius")))
        .or_else(|| env("XDG_CONFIG_HOME").map(|d| d.join("nonius")))
        .or_else(|| env("HOME").map(|d| d.join(".config").join("nonius")))?;
    Some(dir.join("window.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_garbage() {
        let s = Settings {
            tabs: vec![
                PathBuf::from(r"C:\Users\Vova\Проект"),
                PathBuf::from(r"D:\Станок"),
            ],
            active: 1,
            xray: false,
            edges: true,
            wheel_reverse: true,
        };
        assert_eq!(Settings::parse(&s.render()), s);
        assert_eq!(Settings::parse("мусор\n=\nxray\n"), Settings::default());
    }

    #[test]
    fn old_last_project_becomes_a_tab() {
        let s = Settings::parse("last_project=C:\\Проект\nxray=1\n");
        assert_eq!(s.tabs, vec![PathBuf::from("C:\\Проект")]);
    }
}
