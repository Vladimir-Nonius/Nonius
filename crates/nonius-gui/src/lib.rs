//! Окно Nonius: список деталей, выбор версий, 3D-вид с подсветкой изменений.
//!
//! Окно не знает про git и OpenCascade: детали, версии и сравнения приходят
//! через [`source::ProjectSource`]. Файлы: `app/` — окно и панели,
//! `view/` — 3D-вид, `texts.rs` — все надписи.

mod app;
pub mod source;
pub mod texts;
pub mod view;

pub use app::{drafts_dir, App, Options, Settings, SystemChooser};

use eframe::egui;

/// Показать ошибку человеку, у которого нет консоли (окно запущено двойным
/// щелчком). В Windows — системное окно сообщения, иначе — в консоль.
pub fn show_error(text: &str) {
    #[cfg(windows)]
    {
        let _ = rfd::MessageDialog::new()
            .set_title(texts::texts().app_title)
            .set_description(text)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
    #[cfg(not(windows))]
    eprintln!("{text}");
}

/// Открыть окно и ждать, пока его закроют.
pub fn run(options: Options) -> anyhow::Result<()> {
    let t = texts::texts();
    let settings = Settings::load();
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(t.app_title)
            .with_app_id("nonius")
            .with_inner_size([1280.0, 800.0])
            .with_maximized(true)
            .with_min_inner_size([760.0, 480.0])
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        t.app_title,
        native,
        Box::new(|cc| {
            cc.egui_ctx.set_theme(egui::Theme::Light);
            // Стрелки и знаки, которых нет в основном шрифте, — из моноширинного.
            let mut fonts = egui::FontDefinitions::default();
            if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                list.push("Hack".to_owned());
            }
            // Знака диаметра «⌀» (в словах об изменениях) нет ни в одном
            // встроенном шрифте — берём системный шрифт знаков, если он есть.
            if let Some(bytes) = symbols_font() {
                fonts.font_data.insert(
                    "symbols".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                for family in fonts.families.values_mut() {
                    family.push("symbols".to_owned());
                }
            }
            cc.egui_ctx.set_fonts(fonts);
            let render = cc.wgpu_render_state.clone();
            Ok(Box::new(app::App::new(
                &cc.egui_ctx,
                render,
                options,
                settings,
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{} ({e})", t.window_failed))
}

/// Системный шрифт со знаками (Windows: Segoe UI Symbol). Нет — `None`:
/// знак нарисуется квадратиком, но окно работает.
fn symbols_font() -> Option<Vec<u8>> {
    let windows = std::env::var_os("WINDIR")?;
    std::fs::read(
        std::path::Path::new(&windows)
            .join("Fonts")
            .join("seguisym.ttf"),
    )
    .ok()
}
