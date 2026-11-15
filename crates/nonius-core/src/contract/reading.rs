//! Контракты чтения родного файла CAD: беглый взгляд на форму
//! ([`glance`]) и замысел ([`design_source`]). Оба умеют отвечать не
//! целиком — проверяется, что каждый пропуск записан.

use std::collections::HashSet;
use std::path::Path;

use super::{finish, manifest, Violations};
use crate::design::{Doubt, Gap};
use crate::extension::{DesignSource, ExtensionPoint, ShapeGlance};

/// Пропуски записаны как следует: у каждого есть код и слова.
fn gap_problems(gaps: &[Gap]) -> Violations {
    let bad = |g: &&Gap| g.code.trim().is_empty() || g.note.trim().is_empty();
    match gaps.iter().filter(bad).count() {
        0 => Vec::new(),
        n => vec![format!("пропусков без кода или без слов: {n}")],
    }
}

/// Беглый взгляд на форму: берёт образец, не берёт чужие файлы, числа
/// корректны (есть объём и габариты), повторяемы, образец не меняется.
/// Неполный ответ (`glance_partial`) согласован со строгим: те же числа, а
/// если строгий отказал — причина записана пропуском.
pub fn glance<G: ShapeGlance>(src: &G, sample: &Path) -> Result<(), Violations> {
    let mut v = manifest(src, ExtensionPoint::Glance)
        .err()
        .unwrap_or_default();
    if !src.can_glance(sample) {
        v.push(format!("не берётся за образец {}", sample.display()));
        return finish(v);
    }
    if src.can_glance(Path::new("contract-check.nonius-unknown-format")) {
        v.push("берётся за файл неизвестного формата".into());
    }
    let before = std::fs::read(sample).ok();
    match (src.glance(sample), src.glance(sample)) {
        (Ok(a), Ok(b)) => {
            if !a.is_valid() {
                v.push("числа формы некорректны".into());
            }
            if a.volume_mm3.is_none_or(|x| x <= 0.0) || a.bbox.is_none() {
                v.push("нет объёма или габаритов: сравнивать нечем".into());
            }
            if a != b {
                v.push("два взгляда на один файл дали разные числа".into());
            }
        }
        (Err(e), _) | (_, Err(e)) => v.push(format!("ошибка на образце: {e}")),
    }
    v.extend(partial_glance_problems(src, sample));
    if std::fs::read(sample).ok() != before {
        v.push("образец изменился".into());
    }
    finish(v)
}

/// Неполный ответ беглого взгляда против строгого — на любом файле, за
/// который модуль берётся: и на хорошем, и на таком, где строгий отказал.
pub fn partial_glance_problems<G: ShapeGlance>(src: &G, file: &Path) -> Violations {
    let mut v = Vec::new();
    let strict = src.glance(file);
    match (src.glance_partial(file), src.glance_partial(file)) {
        (Ok(a), Ok(b)) => {
            if a != b {
                v.push("два неполных взгляда на один файл дали разный ответ".into());
            }
            if !a.value.is_valid() {
                v.push("числа неполного ответа некорректны".into());
            }
            v.extend(gap_problems(&a.gaps));
            match strict {
                Ok(exact) if exact != a.value => {
                    v.push("неполный ответ расходится со строгим".into())
                }
                Err(_) if a.is_whole() => {
                    v.push("строгий взгляд отказал, а в неполном ответе пропуск не записан".into())
                }
                _ => {}
            }
        }
        (Err(_), Err(_)) => {
            if strict.is_ok() {
                v.push("строгий взгляд ответил, а неполный отказал".into());
            }
        }
        _ => v.push("неполный взгляд то отвечает, то отказывает".into()),
    }
    v
}

/// Источник замысла: берёт образец, не берёт чужие файлы, ответ повторяем,
/// идентификаторы операций непустые и не повторяются, у пропусков и у
/// признаков спорности есть код и слова, образец не меняется.
pub fn design_source<S: DesignSource>(src: &S, sample: &Path) -> Result<(), Violations> {
    let mut v = manifest(src, ExtensionPoint::Design)
        .err()
        .unwrap_or_default();
    if !src.can_read(sample) {
        v.push(format!("не берётся читать образец {}", sample.display()));
        return finish(v);
    }
    if src.can_read(Path::new("contract-check.nonius-unknown-format")) {
        v.push("берётся читать файл неизвестного формата".into());
    }
    let before = std::fs::read(sample).ok();
    match (src.read_design(sample), src.read_design(sample)) {
        (Ok(a), Ok(b)) => {
            if a != b {
                v.push("два чтения одного файла дали разный результат".into());
            }
            v.extend(gap_problems(&a.gaps));
            let mut seen = HashSet::new();
            for op in &a.value.operations {
                if op.id.is_empty() {
                    v.push(format!("у операции «{}» пустой id", op.name));
                } else if !seen.insert(op.id.as_str()) {
                    v.push(format!("id {} повторяется", op.id));
                }
            }
            let unnamed = |d: &&Doubt| d.code.trim().is_empty() || d.note.trim().is_empty();
            if a.value.doubts.iter().any(|d| unnamed(&d)) {
                v.push("признак спорности без кода или без слов".into());
            }
            let active = a.value.active_configuration.as_ref();
            if active.is_some_and(|name| !a.value.configurations.contains(name)) {
                v.push("активной конфигурации нет в списке конфигураций".into());
            }
            if a.value
                .saved_mass_kg
                .is_some_and(|m| !m.is_finite() || m < 0.0)
            {
                v.push("сохранённая масса некорректна".into());
            }
        }
        (Err(e), _) | (_, Err(e)) => v.push(format!("ошибка чтения образца: {e}")),
    }
    if std::fs::read(sample).ok() != before {
        v.push("образец изменился".into());
    }
    finish(v)
}
