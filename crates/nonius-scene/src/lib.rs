//! Модель сцены Nonius: что показывается в 3D.
//!
//! Сцена — набор **мест**: кусков материала с ролью (без изменений,
//! добавлено, убрано, сдвинуто), сеткой и объёмом. Модули-отрисовщики
//! делают сцену из любого CAD-формата, окно её показывает. Ни те, ни другие
//! не знают друг о друге: между ними только этот формат.
//!
//! На диске сцена — стандартный glTF 2.0 (`.glb`), см. [`Scene::to_glb`] и
//! [`Scene::from_glb`]. Формат — публичный контракт с номером версии
//! [`FORMAT_VERSION`]; проверка — [`contract`].
//!
//! Единицы — мм, ось Z смотрит вверх, как в CAD.

pub mod contract;
mod glb;

use std::fmt;

pub use glb::SceneError;

/// Номер формата сцены в `.glb` (`asset.extras.nonius_scene`).
///
/// Новые поля добавляются так, чтобы старые программы их пропускали.
/// Несовместимое изменение повышает номер; старые номера читаются всегда.
pub const FORMAT_VERSION: u32 = 1;

/// Роль места: что с этим материалом произошло.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    /// Материал есть в обеих версиях (или деталь показана без сравнения).
    Unchanged,
    /// Материал добавлен.
    Added,
    /// Материал убран.
    Removed,
    /// Сдвинуто без изменения формы.
    Moved,
}

impl Role {
    /// Все роли в порядке показа.
    pub const ALL: [Role; 4] = [Role::Unchanged, Role::Added, Role::Removed, Role::Moved];

    /// Имя в файле: `unchanged`, `added`, `removed`, `moved`.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Unchanged => "unchanged",
            Role::Added => "added",
            Role::Removed => "removed",
            Role::Moved => "moved",
        }
    }

    /// Роль по имени из файла. Незнакомое имя — `None`.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == text)
    }

    /// Цвет sRGB, общий для картинок, 3D и интерфейса.
    pub fn color(self) -> [u8; 3] {
        match self {
            Role::Unchanged => [176, 182, 190],
            Role::Added => [34, 170, 76],
            Role::Removed => [214, 48, 49],
            Role::Moved => [240, 180, 20],
        }
    }

    /// Изменение ли это (всё, кроме «без изменений»).
    pub fn is_change(self) -> bool {
        self != Role::Unchanged
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Треугольная сетка: вершины в мм и тройки индексов.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    /// Вершины, мм.
    pub positions: Vec<[f32; 3]>,
    /// Треугольники — индексы вершин.
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    /// Пустая ли сетка.
    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    /// Габариты вершин.
    pub fn bounds(&self) -> Option<Bounds> {
        Bounds::of_points(self.positions.iter().copied())
    }
}

/// Габаритный прямоугольный короб, мм.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// Минимальный угол.
    pub min: [f32; 3],
    /// Максимальный угол.
    pub max: [f32; 3],
}

impl Bounds {
    /// Габариты точек; без точек — `None`.
    pub fn of_points(points: impl IntoIterator<Item = [f32; 3]>) -> Option<Self> {
        let mut it = points.into_iter();
        let first = it.next()?;
        let mut b = Bounds {
            min: first,
            max: first,
        };
        for p in it {
            b.include(p);
        }
        Some(b)
    }

    /// Расширить, чтобы точка попала внутрь.
    pub fn include(&mut self, p: [f32; 3]) {
        for ((lo, hi), v) in self.min.iter_mut().zip(&mut self.max).zip(p) {
            *lo = lo.min(v);
            *hi = hi.max(v);
        }
    }

    /// Объединение двух коробов.
    pub fn union(self, other: Bounds) -> Bounds {
        let mut b = self;
        b.include(other.min);
        b.include(other.max);
        b
    }

    /// Центр.
    pub fn center(&self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) / 2.0,
            (self.min[1] + self.max[1]) / 2.0,
            (self.min[2] + self.max[2]) / 2.0,
        ]
    }

    /// Длина диагонали.
    pub fn diagonal(&self) -> f32 {
        (0..3)
            .map(|i| (self.max[i] - self.min[i]).powi(2))
            .sum::<f32>()
            .sqrt()
    }
}

/// Место: кусок материала одной роли.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    /// Что произошло с материалом.
    pub role: Role,
    /// Объём, мм³ (0 — неизвестен).
    pub volume_mm3: f64,
    /// Сетка.
    pub mesh: Mesh,
}

/// Сцена: все места детали.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    /// Места в порядке показа.
    pub regions: Vec<Region>,
}

impl Scene {
    /// Есть ли в сцене изменения.
    pub fn has_changes(&self) -> bool {
        self.regions.iter().any(|r| r.role.is_change())
    }

    /// Места изменений (без серого фона) — по ним ходит список «Места изменений».
    pub fn changes(&self) -> impl Iterator<Item = &Region> {
        self.regions.iter().filter(|r| r.role.is_change())
    }

    /// Места одной роли.
    pub fn regions_of(&self, role: Role) -> impl Iterator<Item = &Region> {
        self.regions.iter().filter(move |r| r.role == role)
    }

    /// Суммарный объём роли, мм³.
    pub fn volume_of(&self, role: Role) -> f64 {
        self.regions_of(role).map(|r| r.volume_mm3).sum()
    }

    /// Габариты всей сцены.
    pub fn bounds(&self) -> Option<Bounds> {
        self.regions
            .iter()
            .filter_map(|r| r.mesh.bounds())
            .reduce(Bounds::union)
    }
}
