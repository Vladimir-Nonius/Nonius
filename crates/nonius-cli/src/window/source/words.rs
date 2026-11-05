//! Что окно говорит человеку: ошибки без подсказок командной строки, масса
//! и почему её нет.

use nonius_app::Mass;
use nonius_gui::source::MassEntry;

/// Сказать словами о том, что в командной строке было бы подсказкой.
pub(super) fn in_words(e: nonius_app::Error) -> anyhow::Error {
    use nonius_app::Error;
    use nonius_core::project::PROJECT_FILE;
    use nonius_core::Error as Core;
    match e {
        Error::Core(Core::NotAProject(path)) => anyhow::anyhow!(
            "в папке {} нет проекта Nonius. Выберите другую папку или создайте проект",
            path.display()
        ),
        Error::Core(Core::AlreadyExists(path)) if path.ends_with(PROJECT_FILE) => {
            anyhow::anyhow!("в этой папке уже есть проект: откройте его")
        }
        Error::Core(Core::AlreadyExists(path)) if !path.join("meta.yaml").is_file() => {
            anyhow::anyhow!(
                "папка {} уже есть: переименуйте или уберите её и повторите",
                path.display()
            )
        }
        Error::Core(Core::AlreadyExists(_)) => anyhow::anyhow!("такая деталь уже есть"),
        // Хранилище отдаёт ту же ошибку ядра, завёрнутую в свою.
        Error::Core(Core::OldLayout(_))
        | Error::Store(nonius_app::StoreError::Core(Core::OldLayout(_))) => {
            anyhow::anyhow!("проект старого формата: менять его можно после обновления (кнопка «Обновить» вверху)")
        }
        Error::Core(Core::InsideItem(_))
        | Error::Store(nonius_app::StoreError::Core(Core::InsideItem(_))) => {
            anyhow::anyhow!("внутрь детали класть нельзя: выберите папку")
        }
        Error::Store(nonius_app::StoreError::Identity) => {
            anyhow::anyhow!("не задано имя автора версий: перезапустите Nonius, он спросит имя")
        }
        Error::Store(nonius_app::StoreError::Dirty(part)) => {
            anyhow::anyhow!("у «{part}» есть несохранённые изменения: сначала сохраните версию")
        }
        Error::Store(nonius_app::StoreError::Core(Core::AlreadyExists(_))) => {
            anyhow::anyhow!("такая деталь уже есть")
        }
        e => e.into(),
    }
}

/// То же для ошибки, уже ставшей общей.
pub(in crate::window) fn plainly(e: anyhow::Error) -> anyhow::Error {
    match e.downcast() {
        Ok(e) => in_words(e),
        Err(e) => e,
    }
}

/// Паспорт → материал и масса для окна.
pub(super) fn mass_entry(p: nonius_app::Passport) -> MassEntry {
    let no_mass = match &p.mass {
        Mass::Computed { .. } | Mass::FromCad { .. } => None,
        Mass::NoMaterial => Some("материал не задан".to_string()),
        Mass::NoDensity { material } => Some(format!(
            "плотность «{material}» неизвестна — выберите «Свой материал…» и введите её"
        )),
        Mass::NoVolume => Some("объём не измерен: в модели нет геометрии".to_string()),
    };
    MassEntry {
        material: p.meta.material.clone(),
        density_kg_m3: p.meta.density_kg_m3,
        mass_kg: p.mass.kg(),
        no_mass,
    }
}
