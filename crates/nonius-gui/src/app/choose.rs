//! Системные окна выбора файла и папки Windows. Там, где их нет, папка берётся
//! из поля ввода, картинка кладётся в текущую папку, а модель перетаскивают.

use std::path::PathBuf;

use crate::source::Chooser;

#[cfg(windows)]
use super::file_menu::MODEL_FILES;
#[cfg(windows)]
use crate::texts::texts;

/// Настоящие окна выбора.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemChooser;

#[cfg(windows)]
impl Chooser for SystemChooser {
    fn folder(&self, _typed: &str) -> Option<PathBuf> {
        rfd::FileDialog::new().pick_folder()
    }

    fn models(&self) -> Vec<PathBuf> {
        rfd::FileDialog::new()
            .add_filter(texts().model_files, &MODEL_FILES)
            .pick_files()
            .unwrap_or_default()
    }

    fn step(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("STEP", &["step", "stp"])
            .pick_file()
    }

    fn files(&self) -> Vec<PathBuf> {
        rfd::FileDialog::new().pick_files().unwrap_or_default()
    }

    fn picture(&self, name: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_file_name(name)
            .add_filter("PNG", &["png"])
            .save_file()
    }

    fn cursor(&self) -> Option<(f32, f32)> {
        match mouse_position::mouse_position::Mouse::get_mouse_position() {
            mouse_position::mouse_position::Mouse::Position { x, y } => Some((x as f32, y as f32)),
            mouse_position::mouse_position::Mouse::Error => None,
        }
    }
}

#[cfg(not(windows))]
impl Chooser for SystemChooser {
    fn folder(&self, typed: &str) -> Option<PathBuf> {
        super::looks_like_dir(typed).then(|| typed.trim().into())
    }

    fn models(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn step(&self) -> Option<PathBuf> {
        None
    }

    fn files(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn picture(&self, name: &str) -> Option<PathBuf> {
        std::env::current_dir().ok().map(|d| d.join(name))
    }
}
