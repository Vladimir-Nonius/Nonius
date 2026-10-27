//! Окно Nonius.
pub fn hello() -> &'static str {
    eframe::egui::special_emojis::GITHUB.to_string().leak()
}
