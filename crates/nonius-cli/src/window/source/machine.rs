//! Компьютер для окна: есть ли git и LFS, имя автора версий.

use anyhow::Result;
use nonius_gui::source::{Machine, Readiness};

/// Компьютер для окна: git, LFS, имя автора.
pub(in crate::window) struct ThisMachine;

impl Machine for ThisMachine {
    fn readiness(&self) -> Readiness {
        let r = nonius_app::readiness();
        Readiness {
            git: r.git,
            lfs: r.lfs,
            author: r.author,
        }
    }

    fn set_author(&self, name: &str) -> Result<()> {
        Ok(nonius_app::set_author(name)?)
    }
}
