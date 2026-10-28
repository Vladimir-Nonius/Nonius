//! `nonius material` и `nonius materials`: материал, плотность, масса.

use anyhow::Result;
use nonius_app::{App, Mass, MaterialChoice, MATERIALS};
use nonius_core::meta::PartMeta;

/// `nonius material`: без материала — показать паспорт, иначе назначить.
pub fn material(
    app: &App,
    part: &str,
    material: Option<&str>,
    density: Option<f64>,
    remove: bool,
) -> Result<()> {
    let passport = match (material, remove) {
        (_, true) => app.set_material(part, None)?,
        (Some(name), false) => app.set_material(
            part,
            Some(MaterialChoice {
                name: name.to_string(),
                density_kg_m3: density,
            }),
        )?,
        (None, false) => app.passport(part, None)?,
    };
    let meta = &passport.meta;
    println!("«{}»", passport.item);
    match (&meta.material, meta.density_kg_m3) {
        (Some(m), Some(d)) => println!("Материал: {m}, плотность {d} кг/м³"),
        (Some(m), None) => println!("Материал: {m}"),
        (None, _) => println!("Материал: не задан"),
    }
    if let Some(v) = meta.geometry.volume_mm3 {
        println!("Объём: {v:.1} мм³");
    }
    print_mass(meta);
    if material.is_some() || remove {
        println!("Зафиксируйте версию: nonius commit {part} -m \"материал\"");
    }
    Ok(())
}

/// Строка о массе: число или почему его нет.
pub fn print_mass(meta: &PartMeta) {
    match Mass::of(meta) {
        Mass::Computed { kg, density_kg_m3 } => {
            println!("Масса: {kg:.3} кг (объём × {density_kg_m3} кг/м³)")
        }
        Mass::FromCad { kg } => println!("Масса: {kg:.3} кг (из CAD)"),
        Mass::NoMaterial => {
            println!("Масса: не посчитана — не задан материал (nonius material <деталь> Ст3)")
        }
        Mass::NoDensity { material } => println!(
            "Масса: не посчитана — плотность «{material}» неизвестна, задайте: --density <кг/м³>"
        ),
        Mass::NoVolume => println!("Масса: не посчитана — объём не измерен"),
    }
}

/// `nonius materials`.
pub fn materials() {
    println!("Справочник материалов (плотность, кг/м³):");
    for m in MATERIALS {
        let also = if m.aliases.is_empty() {
            String::new()
        } else {
            format!("  ({})", m.aliases.join(", "))
        };
        println!("  {:<18} {:>6}{also}", m.name, m.density_kg_m3);
    }
    println!("Нет нужного — задайте сами: nonius material <деталь> <материал> --density <кг/м³>");
}
