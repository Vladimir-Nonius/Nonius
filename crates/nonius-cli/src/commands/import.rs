//! `nonius import`: положить файл модели в деталь и обновить паспорт.

use std::path::Path;

use anyhow::Result;
use nonius_app::App;

use crate::commands::passport::print_mass;

pub fn import(app: &App, file: &Path, part: Option<&str>) -> Result<()> {
    let done = app.import(file, part)?;
    for w in &done.warnings {
        println!("предупреждение: {w}");
    }
    println!("Импортировано в «{}»: {}", done.item.name, done.target);
    if let Some(cad) = &done.meta.source.cad {
        println!("Источник: {cad}");
    }
    if let Some(v) = done.meta.geometry.volume_mm3 {
        println!("Объём: {v:.1} мм³");
        print_mass(&done.meta);
    }
    if done.preview.is_some() {
        println!("Превью: preview.png");
    }
    println!(
        "Зафиксируйте версию: nonius commit {} -m \"что изменилось\"",
        done.item.name
    );
    Ok(())
}
