//! Контрактная проверка сцены. Каждый модуль-отрисовщик прогоняет её в своих
//! тестах: сцена, прошедшая проверку, показывается любым окном Nonius.

use crate::{Scene, FORMAT_VERSION};

/// Нарушения контракта — тексты для человека.
pub type Violations = Vec<String>;

/// Проверить сцену в памяти.
pub fn scene(scene: &Scene) -> Result<(), Violations> {
    let mut v = Violations::new();
    if scene.regions.is_empty() {
        v.push("в сцене нет ни одного места".into());
    }
    for (n, r) in scene.regions.iter().enumerate() {
        let at = format!("место {} ({})", n + 1, r.role);
        if r.mesh.is_empty() {
            v.push(format!("{at}: пустая сетка"));
        }
        let count = r.mesh.positions.len() as u32;
        if r.mesh
            .triangles
            .iter()
            .any(|t| t.iter().any(|&i| i >= count))
        {
            v.push(format!("{at}: индекс треугольника за пределами вершин"));
        }
        if r.mesh.positions.iter().flatten().any(|c| !c.is_finite()) {
            v.push(format!("{at}: координата не число"));
        }
        if !(r.volume_mm3.is_finite() && r.volume_mm3 >= 0.0) {
            v.push(format!("{at}: объём {} — должен быть ≥ 0", r.volume_mm3));
        }
    }
    if v.is_empty() {
        Ok(())
    } else {
        Err(v)
    }
}

/// Проверить файл `.glb` от модуля: номер формата, чтение и сама сцена.
/// Возвращает прочитанную сцену.
pub fn glb(data: &[u8]) -> Result<Scene, Violations> {
    let gltf = gltf::Gltf::from_slice(data).map_err(|e| vec![format!("не glTF: {e}")])?;
    match crate::glb::format_version(&gltf) {
        None => return Err(vec!["нет номера формата asset.extras.nonius_scene".into()]),
        Some(n) if n == 0 || n > FORMAT_VERSION as u64 => {
            return Err(vec![format!(
                "номер формата {n}, допустимо 1…{FORMAT_VERSION}"
            )])
        }
        Some(_) => {}
    }
    let read = Scene::from_glb(data).map_err(|e| vec![e.to_string()])?;
    scene(&read)?;
    Ok(read)
}
