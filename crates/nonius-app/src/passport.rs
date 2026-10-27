//! Паспорт детали: материал, плотность и масса.
//!
//! Масса считается только при заданном материале: объём × плотность.
//! Нет материала — массы нет, и программа так и говорит. Плотность молча не
//! подставляется: у SolidWorks без материала масса считается при 1000 кг/м³,
//! и такая «масса» вводит в заблуждение.

use nonius_core::meta::PartMeta;
use nonius_core::project::META_FILE;
use nonius_core::{schema, PartName, VersionNumber};

use crate::materials::find_material;
use crate::{App, Error, Result};

/// Паспорт детали в версии или в текущем состоянии.
#[derive(Debug, Clone, PartialEq)]
pub struct Passport {
    /// Деталь.
    pub item: PartName,
    /// Версия; `None` — текущее состояние.
    pub version: Option<VersionNumber>,
    /// Содержимое `meta.yaml`.
    pub meta: PartMeta,
    /// Масса и откуда она взялась.
    pub mass: Mass,
}

/// Масса детали.
#[derive(Debug, Clone, PartialEq)]
pub enum Mass {
    /// Посчитана: объём × плотность материала.
    Computed {
        /// Масса, кг.
        kg: f64,
        /// Плотность, кг/м³.
        density_kg_m3: f64,
    },
    /// Пришла из CAD вместе с материалом (плотность не известна Nonius).
    FromCad {
        /// Масса, кг.
        kg: f64,
    },
    /// Материал не задан — массы нет.
    NoMaterial,
    /// Материал задан, но его плотность неизвестна.
    NoDensity {
        /// Материал.
        material: String,
    },
    /// Объём не измерен (программа без OpenCascade или модель не прочитана).
    NoVolume,
}

impl Mass {
    /// Масса в кг, если известна.
    pub fn kg(&self) -> Option<f64> {
        match self {
            Mass::Computed { kg, .. } | Mass::FromCad { kg } => Some(*kg),
            _ => None,
        }
    }

    /// Понять массу по паспорту.
    pub fn of(meta: &PartMeta) -> Mass {
        let Some(material) = meta.material.as_ref().filter(|m| !m.trim().is_empty()) else {
            return Mass::NoMaterial;
        };
        match (meta.density_kg_m3, meta.geometry.volume_mm3) {
            (Some(d), Some(v)) => Mass::Computed {
                kg: mass_kg(v, d),
                density_kg_m3: d,
            },
            _ => match meta.geometry.mass_kg {
                Some(kg) => Mass::FromCad { kg },
                None if meta.density_kg_m3.is_some() => Mass::NoVolume,
                None => Mass::NoDensity {
                    material: material.clone(),
                },
            },
        }
    }
}

/// Объём в мм³ и плотность в кг/м³ → масса в кг.
fn mass_kg(volume_mm3: f64, density_kg_m3: f64) -> f64 {
    volume_mm3 * 1e-9 * density_kg_m3
}

/// Какой материал назначить.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialChoice {
    /// Название материала.
    pub name: String,
    /// Плотность, кг/м³. `None` — взять из справочника, если материал там есть.
    pub density_kg_m3: Option<f64>,
}

/// Пересчитать массу в паспорте после смены материала или геометрии.
/// Масса, пришедшая из CAD без известной плотности, не трогается.
pub(crate) fn update_mass(meta: &mut PartMeta) {
    if meta.material.is_none() {
        meta.density_kg_m3 = None;
    }
    if let Some(d) = meta.density_kg_m3 {
        meta.geometry.mass_kg = meta.geometry.volume_mm3.map(|v| mass_kg(v, d));
    }
}

impl App {
    /// Паспорт детали: в версии `version` или в текущем состоянии (`None`).
    pub fn passport(&self, name: &str, version: Option<VersionNumber>) -> Result<Passport> {
        let item = self.item(name)?;
        let meta = match version {
            Some(n) => {
                let store = self.store()?;
                let v = store.version(&item, n)?;
                store.meta_at(&item, &v)?
            }
            None => self.project.load_meta(&item)?,
        };
        Ok(Passport {
            mass: Mass::of(&meta),
            item: item.name,
            version,
            meta,
        })
    }

    /// Назначить материал (`None` — убрать). Меняет `meta.yaml` в папке
    /// детали; чтобы изменение попало в историю, сохраните версию.
    pub fn set_material(&self, name: &str, choice: Option<MaterialChoice>) -> Result<Passport> {
        let item = self.item(name)?;
        let mut meta = self.project.load_meta(&item)?;
        match choice {
            None => {
                meta.material = None;
                meta.density_kg_m3 = None;
                meta.geometry.mass_kg = None;
            }
            Some(c) => {
                let material = c.name.trim();
                if material.is_empty() {
                    return self.set_material(name, None);
                }
                let density = match c.density_kg_m3 {
                    Some(d) if !(d.is_finite() && d > 0.0) => return Err(Error::BadDensity(d)),
                    Some(d) => Some(d),
                    None => find_material(material).map(|m| m.density_kg_m3),
                };
                if meta.material.as_deref() != Some(material) {
                    // Старая масса относилась к другому материалу.
                    meta.geometry.mass_kg = None;
                }
                meta.material = Some(material.to_string());
                meta.density_kg_m3 = density;
            }
        }
        update_mass(&mut meta);
        schema::save(&meta, &item.dir.join(META_FILE))?;
        self.passport(name, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nonius_core::meta::ItemKind;

    fn meta(material: Option<&str>, density: Option<f64>, volume: Option<f64>) -> PartMeta {
        let mut m = PartMeta::new(ItemKind::Part);
        m.material = material.map(String::from);
        m.density_kg_m3 = density;
        m.geometry.volume_mm3 = volume;
        m
    }

    #[test]
    fn no_material_means_no_mass() {
        let mut m = meta(None, None, Some(1000.0));
        m.geometry.mass_kg = Some(1.0); // «масса» из CAD при плотности воды
        assert_eq!(Mass::of(&m), Mass::NoMaterial);
    }

    #[test]
    fn mass_is_volume_times_density() {
        // 100 × 100 × 10 мм стали = 0,785 кг
        let m = meta(Some("Ст3"), Some(7850.0), Some(100_000.0));
        let Mass::Computed { kg, density_kg_m3 } = Mass::of(&m) else {
            panic!("{:?}", Mass::of(&m));
        };
        assert!((kg - 0.785).abs() < 1e-9, "{kg}");
        assert_eq!(density_kg_m3, 7850.0);
    }

    #[test]
    fn unknown_density_and_volume_are_explained() {
        let m = meta(Some("Унобтаниум"), None, Some(1.0));
        assert!(matches!(Mass::of(&m), Mass::NoDensity { .. }));
        let m = meta(Some("Ст3"), Some(7850.0), None);
        assert_eq!(Mass::of(&m), Mass::NoVolume);
        let mut m = meta(Some("Ст3"), None, None);
        m.geometry.mass_kg = Some(2.5);
        assert_eq!(Mass::of(&m), Mass::FromCad { kg: 2.5 });
    }

    #[test]
    fn update_mass_follows_volume() {
        let mut m = meta(Some("Ст3"), Some(7850.0), Some(1e6));
        update_mass(&mut m);
        assert!((m.geometry.mass_kg.unwrap() - 7.85).abs() < 1e-9);
        m.material = None;
        update_mass(&mut m);
        assert_eq!(m.density_kg_m3, None);
    }
}
