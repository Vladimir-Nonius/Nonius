//! Склад STEP на диске: `.git/nonius/steps` проекта.
//!
//! STEP из файла CAD даёт модуль перевода (SolidWorks: секунды на деталь, а
//! зависнет — минуты). Чтобы один и тот же STEP не просили дважды — ни в
//! одном проходе, ни завтра, — готовый файл ложится на склад, и все, кому
//! нужен STEP (импорт, разбор папки, слова «что изменилось», «Показать в
//! 3D»), сначала смотрят сюда.
//!
//! **Ключ** — содержимое файла (SHA-256) и отпечаток условий перевода
//! (`Converter::stamp`: версия CAD-системы, её настройки выгрузки, файлы, от
//! которых зависит сборка или деталь «в контексте»). Путь и время файла в
//! ключ не входят: копия детали в другой папке — тот же STEP. Модуль не
//! смог назвать условия — STEP на склад не кладётся.
//!
//! **Что лежит.** `<ключ>.step` — готовый STEP; `<ключ>.refused` — отказ
//! модуля со словами причины («SolidWorks не справился»): пока файл тот же,
//! модуль больше не будят; `<ключ>.empty` рядом со STEP — в нём нет твёрдого
//! тела («тела нет»): точной формы у такой детали не будет.
//!
//! **Предел и очистка.** Склад — кэш: его можно удалить целиком в любой
//! момент (`App::forget_steps` или просто папку). Вырос больше предела
//! ([`LIMIT`], переменная `NONIUS_STEP_STORE_MB`) — самые давно не нужные
//! файлы удаляются, пока не останется три четверти предела. Нужность — время
//! изменения файла: каждое обращение его обновляет.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::App;

/// Предел размера склада, байт: полгигабайта — сотни деталей.
const LIMIT: u64 = 512 * 1024 * 1024;
/// Поколение ключа: изменится способ счёта — старые записи не подойдут.
const GENERATION: &str = "steps1";

/// Ключ записи склада.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StepKey(String);

/// Что нашлось на складе.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Stored {
    /// Готовый STEP.
    Step(PathBuf),
    /// Модуль отказался окончательно: причина словами.
    Refused(String),
}

/// Склад STEP одного проекта.
pub(crate) struct StepStore {
    dir: PathBuf,
    limit: u64,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Отпечаток содержимого файла. Считается каждый раз заново: размеру и
/// времени файла склад не верит (деталь могли пересохранить в ту же
/// секунду), а чтение файла — миллисекунды против секунд SolidWorks.
fn content(file: &Path) -> Option<String> {
    let mut hasher = Sha256::new();
    io::copy(&mut fs::File::open(file).ok()?, &mut hasher).ok()?;
    Some(hex(&hasher.finalize()))
}

impl StepStore {
    /// Склад в рабочей папке программы `work` (`.git/nonius`).
    pub fn at(work: &Path) -> Self {
        let megabytes = std::env::var("NONIUS_STEP_STORE_MB").ok();
        let limit = megabytes.and_then(|m| m.trim().parse::<u64>().ok());
        Self {
            dir: work.join("steps"),
            limit: limit.map_or(LIMIT, |m| m * 1024 * 1024),
        }
    }

    /// Склад с заданным пределом — для тестов.
    #[cfg(test)]
    fn with_limit(work: &Path, limit: u64) -> Self {
        Self {
            dir: work.join("steps"),
            limit,
        }
    }

    /// Ключ файла `file` при условиях перевода `stamp`. `None` — файл не
    /// читается.
    pub fn key(file: &Path, stamp: &str) -> Option<StepKey> {
        let mut conditions = Sha256::new();
        conditions.update(GENERATION.as_bytes());
        conditions.update([0]);
        conditions.update(stamp.as_bytes());
        let conditions = hex(&conditions.finalize());
        Some(StepKey(format!(
            "{}-{}",
            &content(file)?[..40],
            &conditions[..16]
        )))
    }

    fn file(&self, key: &StepKey, kind: &str) -> PathBuf {
        self.dir.join(format!("{}.{kind}", key.0))
    }

    /// Что лежит под ключом. Обращение отмечается: запись «нужна».
    pub fn get(&self, key: &StepKey) -> Option<Stored> {
        let step = self.file(key, "step");
        if step.is_file() {
            touch(&step);
            return Some(Stored::Step(step));
        }
        let refused = self.file(key, "refused");
        let why = fs::read_to_string(&refused).ok()?;
        touch(&refused);
        Some(Stored::Refused(why.trim().to_string()))
    }

    /// Положить готовый STEP. Не вышло — не беда: это кэш.
    pub fn put(&self, key: &StepKey, step: &Path) {
        if !self.ready() {
            return;
        }
        // Сначала под временным именем: оборванная запись не станет «STEP».
        let tmp = self.file(key, "tmp");
        let placed = fs::copy(step, &tmp).and_then(|_| fs::rename(&tmp, self.file(key, "step")));
        if placed.is_err() {
            let _ = fs::remove_file(&tmp);
            return;
        }
        let _ = fs::remove_file(self.file(key, "refused"));
        let _ = fs::remove_file(self.file(key, "empty"));
        self.trim();
    }

    /// Запомнить окончательный отказ модуля.
    pub fn refuse(&self, key: &StepKey, why: &str) {
        if self.ready() {
            let _ = fs::write(self.file(key, "refused"), why);
        }
    }

    /// Запомнить: STEP есть, а твёрдого тела в нём нет.
    pub fn mark_empty(&self, key: &StepKey) {
        if self.file(key, "step").is_file() {
            let _ = fs::write(self.file(key, "empty"), "");
        }
    }

    /// Известно, что тела в STEP этой записи нет.
    pub fn is_empty(&self, key: &StepKey) -> bool {
        self.file(key, "empty").is_file()
    }

    /// Папка склада на месте. `.git` над рабочей папкой не создаём: историю
    /// версий проект заводит сам (см. `intake/memory.rs`).
    fn ready(&self) -> bool {
        if let Some(work) = self.dir.parent() {
            let _ = fs::create_dir(work);
        }
        let _ = fs::create_dir(&self.dir);
        self.dir.is_dir()
    }

    fn entries(&self) -> Vec<(PathBuf, u64, SystemTime)> {
        let Ok(list) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        list.flatten()
            .filter_map(|e| {
                let meta = e.metadata().ok().filter(fs::Metadata::is_file)?;
                Some((e.path(), meta.len(), meta.modified().ok()?))
            })
            .collect()
    }

    /// Сколько байт занято.
    pub fn size(&self) -> u64 {
        self.entries().iter().map(|(_, len, _)| len).sum()
    }

    /// Уложиться в предел: удалить самые давно не нужные записи.
    fn trim(&self) {
        let mut entries = self.entries();
        let mut total: u64 = entries.iter().map(|(_, len, _)| len).sum();
        if total <= self.limit {
            return;
        }
        entries.sort_by_key(|(_, _, at)| *at);
        for (path, len, _) in entries {
            if total <= self.limit / 4 * 3 {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }

    /// Удалить всё. Возвращает, сколько байт освободилось.
    pub fn clear(&self) -> u64 {
        let mut freed = 0;
        for (path, len, _) in self.entries() {
            if fs::remove_file(&path).is_ok() {
                freed += len;
            }
        }
        freed
    }
}

/// Отметить обращение к записи: по времени файла склад решает, что удалять.
fn touch(file: &Path) {
    if let Ok(f) = fs::File::options().write(true).open(file) {
        let _ = f.set_modified(SystemTime::now());
    }
}

impl App {
    /// Склад STEP этого проекта.
    pub(crate) fn steps(&self) -> StepStore {
        StepStore::at(&crate::work_dir(self.project.root()))
    }

    /// Ключ склада для файла CAD: `None` — модуля перевода нет или условия
    /// перевода неизвестны (такой STEP на склад не кладётся).
    pub(crate) fn step_key(&self, file: &Path) -> Option<StepKey> {
        let stamp = self.registry.converter_for(file)?.stamp(file)?;
        StepStore::key(file, &stamp)
    }

    /// Сколько места занимает склад STEP проекта, байт.
    pub fn steps_size(&self) -> u64 {
        self.steps().size()
    }

    /// Очистить склад STEP проекта: готовые STEP и запомненные отказы
    /// модуля перевода. Это кэш — всё получится заново, только медленнее.
    /// Возвращает, сколько байт освободилось.
    pub fn forget_steps(&self) -> u64 {
        self.steps().clear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn key_follows_content_and_conditions_not_the_path() {
        let tmp = work();
        let (a, b) = (
            tmp.path().join("вал.sldprt"),
            tmp.path().join("копия вала.sldprt"),
        );
        fs::write(&a, "раз").unwrap();
        fs::write(&b, "раз").unwrap();
        let key = StepStore::key(&a, "SolidWorks 2025").unwrap();
        assert_eq!(StepStore::key(&b, "SolidWorks 2025"), Some(key.clone()));
        assert_ne!(StepStore::key(&a, "SolidWorks 2026"), Some(key.clone()));
        fs::write(&a, "раз и два").unwrap();
        assert_ne!(StepStore::key(&a, "SolidWorks 2025"), Some(key));
        assert_eq!(StepStore::key(&tmp.path().join("нет"), "x"), None);
    }

    #[test]
    fn step_and_refusals_are_kept() {
        let tmp = work();
        let store = StepStore::at(tmp.path());
        let model = tmp.path().join("вал.sldprt");
        fs::write(&model, "деталь").unwrap();
        let key = StepStore::key(&model, "x").unwrap();
        assert_eq!(store.get(&key), None);

        store.refuse(&key, "SolidWorks дважды не справился");
        assert_eq!(
            store.get(&key),
            Some(Stored::Refused("SolidWorks дважды не справился".into()))
        );

        // Готовый STEP отменяет прежний отказ.
        let step = tmp.path().join("готовый.step");
        fs::write(&step, "ISO-10303-21;").unwrap();
        store.put(&key, &step);
        let Some(Stored::Step(found)) = store.get(&key) else {
            panic!("STEP должен лежать на складе");
        };
        assert_eq!(fs::read_to_string(found).unwrap(), "ISO-10303-21;");
        assert!(!store.is_empty(&key));
        store.mark_empty(&key);
        assert!(store.is_empty(&key));

        assert!(store.size() > 0);
        assert!(store.clear() > 0);
        assert_eq!(store.get(&key), None);
    }

    #[test]
    fn oldest_entries_go_when_the_limit_is_passed() {
        let tmp = work();
        // Предел 280 байт: три записи по 100 не помещаются, убирать — до 210.
        let store = StepStore::with_limit(tmp.path(), 280);
        let step = tmp.path().join("готовый.step");
        fs::write(&step, [b'x'; 100]).unwrap();
        let keys: Vec<StepKey> = (0..3)
            .map(|n| {
                let model = tmp.path().join(format!("{n}.sldprt"));
                fs::write(&model, n.to_string()).unwrap();
                StepStore::key(&model, "x").unwrap()
            })
            .collect();
        let old = SystemTime::now() - std::time::Duration::from_secs(3600);
        store.put(&keys[0], &step);
        let first = store.file(&keys[0], "step");
        let file = fs::File::options().write(true).open(&first).unwrap();
        file.set_modified(old).unwrap();
        drop(file);
        store.put(&keys[1], &step);
        assert!(first.is_file(), "200 байт — в пределе");
        store.put(&keys[2], &step);
        assert_eq!(store.get(&keys[0]), None, "самый старый ушёл");
        assert!(store.get(&keys[1]).is_some());
        assert!(store.get(&keys[2]).is_some());
    }

    #[test]
    fn without_git_folder_nothing_is_stored() {
        let tmp = work();
        // Рабочая папка лежала бы в `.git`, а его нет: склад молчит.
        let store = StepStore::at(&tmp.path().join(".git").join("nonius"));
        let model = tmp.path().join("вал.sldprt");
        fs::write(&model, "деталь").unwrap();
        let key = StepStore::key(&model, "x").unwrap();
        store.put(&key, &model);
        store.refuse(&key, "нет");
        assert_eq!(store.get(&key), None);
        assert!(!tmp.path().join(".git").exists());
    }
}
