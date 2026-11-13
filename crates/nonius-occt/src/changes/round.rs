//! Цилиндрические грани: отверстия, бобышки и скругления.
//!
//! Грани с одной осью и одним радиусом — одно место (CAD режет цилиндр на
//! половинки). Полный оборот — отверстие или бобышка, четверть — скругление.
//! Пары «было — стало» ищутся в три прохода: та же ось (изменился диаметр
//! или глубина), тот же диаметр (сдвинули), остаток — новое и убранное.

use std::f64::consts::PI;

use nonius_core::compare::Change;

use super::text::{dot, length, mm, same_direction, scaled, sub, V};
use super::{line, Face, Side};

/// Цилиндрическое место: все грани одной оси и одного радиуса.
struct Round {
    side: Side,
    inward: bool,
    radius: f64,
    /// Направление оси, одно и то же у встречных осей.
    dir: V,
    /// Ближайшая к началу координат точка оси: у одной прямой она одна.
    foot: V,
    angle: f64,
    /// Границы вдоль оси.
    low: f64,
    high: f64,
    faces: Vec<Face>,
    used: bool,
}

impl Round {
    fn of(f: &Face) -> Self {
        let flip = f
            .dir
            .iter()
            .find(|x| x.abs() > 1e-9)
            .is_some_and(|x| *x < 0.0);
        let sign = if flip { -1.0 } else { 1.0 };
        let dir = scaled(f.dir, sign);
        let along = dot(f.origin, dir);
        let (a, b) = (along + sign * f.span.0, along + sign * f.span.1);
        Self {
            side: f.side,
            inward: f.inward,
            radius: f.radius,
            dir,
            foot: sub(f.origin, scaled(dir, along)),
            angle: f.angle,
            low: a.min(b),
            high: a.max(b),
            faces: vec![*f],
            used: false,
        }
    }

    fn same_axis(&self, other: &Round, tol: f64) -> bool {
        same_direction(self.dir, other.dir) && length(sub(self.foot, other.foot)) <= tol
    }

    fn same_place(&self, other: &Round, tol: f64) -> bool {
        self.side == other.side
            && self.inward == other.inward
            && (self.radius - other.radius).abs() <= tol
            && self.same_axis(other, tol)
    }

    fn join(&mut self, other: Round) {
        self.angle += other.angle;
        self.low = self.low.min(other.low);
        self.high = self.high.max(other.high);
        self.faces.extend(other.faces);
    }

    fn full(&self) -> bool {
        self.angle >= 2.0 * PI - 0.02
    }

    /// Скругление: не больше четверти оборота с запасом.
    fn fillet(&self) -> bool {
        self.angle <= 1.75
    }

    fn length(&self) -> f64 {
        self.high - self.low
    }

    fn diameter(&self) -> String {
        format!("⌀{}", mm(2.0 * self.radius))
    }
}

/// Слова для полного цилиндра: отверстие или бобышка.
struct Words {
    name: &'static str,
    fresh: &'static str,
    gone: &'static str,
    depth: &'static str,
    moved: &'static str,
}

const HOLE: Words = Words {
    name: "Отверстие",
    fresh: "Новое отверстие",
    gone: "Убрано отверстие",
    depth: "глубина",
    moved: "сдвинуто",
};

const BOSS: Words = Words {
    name: "Бобышка",
    fresh: "Новая бобышка",
    gone: "Убрана бобышка",
    depth: "высота",
    moved: "сдвинута",
};

fn groups(faces: &[Face], tol: f64) -> Vec<Round> {
    let mut out: Vec<Round> = Vec::new();
    for f in faces {
        let r = Round::of(f);
        match out.iter_mut().find(|g| g.same_place(&r, tol)) {
            Some(g) => g.join(r),
            None => out.push(r),
        }
    }
    out
}

/// Пары «было — стало» среди свободных мест, ближайшие первыми.
fn pairs(
    all: &[Round],
    pick: impl Fn(&Round) -> bool,
    fits: impl Fn(&Round, &Round) -> Option<f64>,
) -> Vec<(usize, usize)> {
    let free = |side: Side| -> Vec<usize> {
        (0..all.len())
            .filter(|&i| !all[i].used && all[i].side == side && pick(&all[i]))
            .collect()
    };
    let mut found: Vec<(f64, usize, usize)> = Vec::new();
    for &i in &free(Side::Before) {
        for &j in &free(Side::After) {
            if let Some(d) = fits(&all[i], &all[j]) {
                found.push((d, i, j));
            }
        }
    }
    found.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut taken = vec![false; all.len()];
    let mut out = Vec::new();
    for (_, i, j) in found {
        if !taken[i] && !taken[j] {
            taken[i] = true;
            taken[j] = true;
            out.push((i, j));
        }
    }
    out
}

fn full_cylinders(all: &mut [Round], inward: bool, w: &Words, tol: f64, out: &mut Vec<Change>) {
    let pick = |r: &Round| r.full() && r.inward == inward;
    // Та же ось: изменился диаметр или глубина.
    for (i, j) in pairs(all, pick, |b, a| b.same_axis(a, tol).then_some(0.0)) {
        let (b, a) = (&all[i], &all[j]);
        if (b.radius - a.radius).abs() > tol {
            out.push(line(w.name.into(), Some(b.diameter()), Some(a.diameter())));
        } else if (b.length() - a.length()).abs() > tol {
            out.push(line(
                format!("{} {}, {}", w.name, a.diameter(), w.depth),
                Some(format!("{} мм", mm(b.length()))),
                Some(format!("{} мм", mm(a.length()))),
            ));
        }
        all[i].used = true;
        all[j].used = true;
    }
    // Тот же диаметр и то же направление: сдвинули.
    let moved = |b: &Round, a: &Round| {
        ((b.radius - a.radius).abs() <= tol && same_direction(b.dir, a.dir))
            .then(|| length(sub(b.foot, a.foot)))
    };
    for (i, j) in pairs(all, pick, moved) {
        let shift = length(sub(all[i].foot, all[j].foot));
        out.push(line(
            format!(
                "{} {} {} на {} мм",
                w.name,
                all[j].diameter(),
                w.moved,
                mm(shift)
            ),
            None,
            None,
        ));
        all[i].used = true;
        all[j].used = true;
    }
    for r in all.iter_mut().filter(|r| !r.used && pick(r)) {
        let word = match r.side {
            Side::After => w.fresh,
            Side::Before => w.gone,
        };
        out.push(line(format!("{word} {}", r.diameter()), None, None));
        r.used = true;
    }
}

fn fillets(all: &mut [Round], tol: f64, out: &mut Vec<Change>) {
    // Та же ось и тот же радиус: скругление осталось, изменились его края.
    let same = |b: &Round, a: &Round| {
        (b.same_axis(a, tol) && (b.radius - a.radius).abs() <= tol).then_some(0.0)
    };
    for (i, j) in pairs(all, Round::fillet, same) {
        all[i].used = true;
        all[j].used = true;
    }
    let left: Vec<(Side, f64)> = all
        .iter_mut()
        .filter(|r| !r.used && r.fillet())
        .map(|r| {
            r.used = true;
            (r.side, r.radius)
        })
        .collect();
    let of = |side: Side| -> Vec<f64> {
        left.iter()
            .filter(|(s, _)| *s == side)
            .map(|(_, r)| *r)
            .collect()
    };
    let (gone, fresh) = (of(Side::Before), of(Side::After));
    let one_size = |list: &[f64]| list.iter().all(|r| (r - list[0]).abs() <= tol);
    // Все скругления одного радиуса заменены таким же числом другого.
    if !gone.is_empty() && gone.len() == fresh.len() && one_size(&gone) && one_size(&fresh) {
        for _ in &gone {
            out.push(line(
                "Скругление".into(),
                Some(format!("R{}", mm(gone[0]))),
                Some(format!("R{}", mm(fresh[0]))),
            ));
        }
        return;
    }
    for r in gone {
        out.push(line(format!("Убрано скругление R{}", mm(r)), None, None));
    }
    for r in fresh {
        out.push(line(format!("Новое скругление R{}", mm(r)), None, None));
    }
}

/// Описать цилиндрические грани. Возвращает грани, которым слов не нашлось.
pub(super) fn describe(faces: &[Face], tol: f64, out: &mut Vec<Change>) -> Vec<Face> {
    let mut all = groups(faces, tol);
    full_cylinders(&mut all, true, &HOLE, tol, out);
    full_cylinders(&mut all, false, &BOSS, tol, out);
    fillets(&mut all, tol, out);
    all.into_iter()
        .filter(|r| !r.used)
        .flat_map(|r| r.faces)
        .collect()
}
