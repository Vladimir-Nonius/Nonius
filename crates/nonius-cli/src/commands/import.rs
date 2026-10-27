//! `nonius import`: положить файл модели в деталь и обновить паспорт.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use nonius_core::extension::GeometrySource;
use nonius_core::meta::ItemKind;
use nonius_core::project::META_FILE;
use nonius_core::{schema, PartName, Project};
use nonius_step::StepHeaderSource;

use crate::registry;

pub fn import(project: &Project, file: &Path, part: Option<&str>) -> Result<()> {
    if !file.is_file() {
        bail!("{}: файл не найден", file.display());
    }
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let (target, kind) = match ext.as_str() {
        "step" | "stp" => ("model.step", ItemKind::Part),
        "sldprt" => ("model.sldprt", ItemKind::Part),
        "sldasm" => ("model.sldasm", ItemKind::Assembly),
        _ => bail!(
            "{}: неизвестный формат (поддерживаются STEP, SLDPRT, SLDASM)",
            file.display()
        ),
    };
    let name = match part {
        Some(p) => p.to_string(),
        None => file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    let name = PartName::new(name.as_str())
        .with_context(|| "имя детали из имени файла не подходит, задайте его: --part <имя>")?;

    let item = match project.find_item(&name)? {
        Some(i) => i,
        None => project.create_item(kind, &name)?,
    };
    let dest = item.dir.join(target);
    fs::copy(file, &dest)
        .with_context(|| format!("не удалось скопировать в {}", dest.display()))?;

    let mut meta = project.load_meta(&item)?;
    if let Some(src) = registry().geometry_source_for(file) {
        // Геометрию прочитать не удалось (пустой или повреждённый STEP) —
        // импорт не ломаем: берём сведения из заголовка и предупреждаем.
        let read = match src.read(file) {
            Ok(m) => m,
            Err(e) if src.manifest().id != StepHeaderSource::ID => {
                println!("предупреждение: геометрия не прочитана: {e}");
                StepHeaderSource::new().read(file)?
            }
            Err(e) => return Err(e.into()),
        };
        meta.geometry = read.geometry;
        meta.source = read.source;
        if read.material.is_some() {
            meta.material = read.material;
        }
        meta.properties.extend(read.properties);
    } else if meta.source.cad.is_none() {
        meta.source.cad = Some("SolidWorks".into());
    }
    schema::save(&meta, &item.dir.join(META_FILE))?;

    println!("Импортировано в «{}»: {target}", item.name);
    if let Some(cad) = &meta.source.cad {
        println!("Источник: {cad}");
    }
    if let Some(v) = meta.geometry.volume_mm3 {
        println!("Объём: {v:.1} мм³");
    }
    if target == "model.step" {
        match nonius_occt::preview_png(&dest, &item.dir.join("preview.png")) {
            Ok(true) => println!("Превью: preview.png"),
            Ok(false) => {}
            Err(e) => println!("предупреждение: превью не сделано: {e}"),
        }
    }
    println!(
        "Зафиксируйте версию: nonius commit {} -m \"что изменилось\"",
        item.name
    );
    Ok(())
}
