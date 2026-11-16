//! Импорт файла модели: положить файл в деталь и обновить паспорт.
//!
//! Файл SolidWorks (`.sldprt`, `.sldasm`) хранится как есть, но форму, массу
//! и сравнение Nonius считает по STEP. Если подключён модуль перевода
//! (точка `Converter`, например `nonius-solidworks`), импорт сам получает
//! свежий STEP и кладёт его рядом как `model.step`. Нет модуля или перевод
//! не удался — файл всё равно импортируется, а в предупреждениях сказано,
//! что STEP остался прежним.

use std::fs;
use std::path::{Path, PathBuf};

use nonius_core::extension::ConvertAsk;
use nonius_core::meta::{ItemKind, PartMeta};
use nonius_core::project::{Item, META_FILE};
use nonius_core::{schema, PartName};

use crate::materials::find_material;
use crate::passport::update_mass;
use crate::steps::{StepKey, Stored};
use crate::{App, Error, NoProgress, Progress, Result};

/// Формат экспорта «превью детали»: модуль, который его объявляет, рисует
/// `preview.png` для списка деталей.
pub const PREVIEW_FORMAT: &str = "preview";

/// Файл SolidWorks заменён, а STEP рядом остался старый.
const STEP_BEHIND: &str =
    "форма в 3D и масса — по прежнему STEP: чтобы обновить их, добавьте и новый STEP этой детали";

/// STEP, полученный модулем перевода: лежит во временной папке до тех пор,
/// пока импорт не положит его в деталь.
pub(crate) struct Converted {
    _dir: tempfile::TempDir,
    pub(crate) step: PathBuf,
    pub(crate) read: Option<PartMeta>,
    /// Ключ записи на складе STEP, если STEP туда лёг.
    pub(crate) key: Option<StepKey>,
}

/// Пожелания к переводу в STEP: кто просит.
pub(crate) struct Wish<'a> {
    /// STEP ждёт человек: в очереди модуля перевода — вперёд фоновых.
    pub urgent: bool,
    /// Место в пачке: «3 из 27».
    pub place: Option<(usize, usize)>,
    /// Куда писать строку хода и откуда узнать об отмене.
    pub progress: &'a dyn Progress,
}

impl<'a> Wish<'a> {
    /// Просьба человека.
    pub fn urgent(progress: &'a dyn Progress) -> Self {
        Self {
            urgent: true,
            place: None,
            progress,
        }
    }
}

/// Итог импорта.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// Деталь (создаётся, если её не было).
    pub item: Item,
    /// Имя файла модели в папке детали: `model.step`, `model.sldprt`, …
    pub target: &'static str,
    /// Паспорт после импорта.
    pub meta: PartMeta,
    /// Превью, если его удалось нарисовать.
    pub preview: Option<PathBuf>,
    /// Предупреждения: импорт прошёл, но не всё получилось.
    pub warnings: Vec<String>,
}

impl App {
    /// Добавить файл модели в деталь `part` (по умолчанию — имя файла без
    /// расширения; деталь создаётся, если её нет) и обновить паспорт.
    /// Материал и плотность сохраняются, масса пересчитывается по новому объёму.
    pub fn import(&self, file: &Path, part: Option<&str>) -> Result<Imported> {
        self.import_with(file, part, &NoProgress)
    }

    /// То же со строкой хода и отменой: STEP файла SolidWorks может
    /// потребовать ожидания (очередь к SolidWorks, его перезапуск).
    pub fn import_with(
        &self,
        file: &Path,
        part: Option<&str>,
        progress: &dyn Progress,
    ) -> Result<Imported> {
        self.project.writable()?;
        if !file.is_file() {
            return Err(Error::FileNotFound(file.to_path_buf()));
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
            _ => return Err(Error::UnknownFormat(file.to_path_buf())),
        };
        let name = match part {
            Some(p) => p.to_string(),
            None => file
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };
        let name = PartName::new(name.as_str()).map_err(Error::BadItemName)?;

        // Сначала прочитать, потом трогать проект: битый файл не создаёт
        // деталь и не портит рабочую модель.
        let mut warnings = Vec::new();
        let converted = if target == "model.step" {
            None
        } else {
            // Импорта ждёт человек: в очереди к SolidWorks — вперёд фонового.
            self.convert(file, &Wish::urgent(progress), &mut warnings)
        };
        let read = match &converted {
            Some(c) => c.read.clone(),
            None => self.read_geometry(file, &mut warnings)?,
        };
        let found = self.project.find_item(&name)?;
        let fresh = found.is_none();
        let item = match found {
            Some(i) => i,
            None => self.project.create_item(kind, &name)?,
        };
        let placed = replace_file(file, &item.dir.join(target)).and_then(|()| match &converted {
            Some(c) => replace_file(&c.step, &item.dir.join("model.step")),
            None => Ok(()),
        });
        if let Err(e) = placed {
            if fresh {
                let _ = fs::remove_dir_all(&item.dir);
            }
            return Err(e);
        }

        let mut meta = self.project.load_meta(&item)?;
        let step_is_fresh = target == "model.step" || converted.is_some();
        if !step_is_fresh && item.dir.join("model.step").is_file() {
            warnings.push(STEP_BEHIND.to_string());
        }
        if let Some(read) = read {
            meta.geometry = read.geometry;
            meta.source = read.source;
            if read.material.is_some() && read.material != meta.material {
                // Новый материал из CAD: плотность — из справочника, если он там есть.
                meta.density_kg_m3 = read
                    .material
                    .as_deref()
                    .and_then(find_material)
                    .map(|m| m.density_kg_m3);
                meta.material = read.material;
            }
            meta.properties.extend(read.properties);
        } else if meta.source.cad.is_none() {
            meta.source.cad = Some("SolidWorks".into());
        }
        update_mass(&mut meta);
        schema::save(&meta, &item.dir.join(META_FILE))?;

        let preview = if step_is_fresh {
            self.make_preview(&item, &mut warnings)
        } else {
            None
        };
        Ok(Imported {
            item,
            target,
            meta,
            preview,
            warnings,
        })
    }

    /// Получить STEP файла CAD модулем перевода и прочитать его паспорт.
    /// `None` — модуля нет или перевод не удался (причина — в предупреждениях):
    /// импорт идёт дальше по-старому, без свежего STEP.
    ///
    /// Сначала — склад STEP (`steps.rs`): готовый STEP того же содержимого
    /// модуль второй раз не просят, а запомненный отказ («SolidWorks не
    /// справился») сразу становится предупреждением.
    pub(crate) fn convert(
        &self,
        file: &Path,
        wish: &Wish,
        warnings: &mut Vec<String>,
    ) -> Option<Converted> {
        let converter = self.registry.converter_for(file)?;
        let failed = |why: String, warnings: &mut Vec<String>| {
            warnings.push(format!("STEP не получен: {why}"));
            None
        };
        let dir = match tempfile::tempdir() {
            Ok(d) => d,
            Err(e) => return failed(format!("временная папка: {e}"), warnings),
        };
        let step = dir.path().join("model.step");
        let store = self.steps();
        let key = self.step_key(file);
        let stored = match key.as_ref().and_then(|k| store.get(k)) {
            Some(Stored::Refused(why)) => {
                let kept = "так было и в прошлый раз, а файл с тех пор не менялся";
                return failed(format!("{why} ({kept})"), warnings);
            }
            // Со склада — копией: запись могут убрать, пока STEP в работе.
            Some(Stored::Step(found)) => fs::copy(found, &step).is_ok(),
            None => false,
        };
        if !stored {
            let progress = wish.progress;
            let ask = ConvertAsk {
                urgent: wish.urgent,
                place: wish.place,
                notify: &|what| progress.step(what),
                cancelled: &|| progress.cancelled(),
            };
            let mut done = None;
            progress.outside(&mut || done = Some(converter.to_step_asked(file, &step, &ask)));
            match done.unwrap_or_else(|| converter.to_step_asked(file, &step, &ask)) {
                Ok(()) => {
                    if let Some(key) = &key {
                        store.put(key, &step);
                    }
                }
                Err(e) => {
                    if let (nonius_core::Error::Refused { message, .. }, Some(key)) = (&e, &key) {
                        store.refuse(key, message);
                    }
                    return failed(e.to_string(), warnings);
                }
            }
        }
        match self.read_geometry(&step, warnings) {
            Ok(read) => Some(Converted {
                _dir: dir,
                step,
                read,
                key,
            }),
            Err(e) => failed(e.to_string(), warnings),
        }
    }

    /// Прочитать паспорт подходящим модулем. Если первый модуль не справился
    /// (пустой или повреждённый STEP), пробуем следующий и предупреждаем.
    pub(crate) fn read_geometry(
        &self,
        file: &Path,
        warnings: &mut Vec<String>,
    ) -> Result<Option<PartMeta>> {
        let mut last_err = None;
        for src in self.registry.geometry_sources_for(file) {
            match src.read(file) {
                Ok(m) => return Ok(Some(m)),
                Err(e) => {
                    warnings.push(format!("геометрия не прочитана: {e}"));
                    last_err = Some(e);
                }
            }
        }
        match last_err {
            Some(e) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Превью детали — модулем экспорта в формат [`PREVIEW_FORMAT`], если
    /// такой подключён. Модуль кладёт файл в папку детали.
    fn make_preview(&self, item: &Item, warnings: &mut Vec<String>) -> Option<PathBuf> {
        let exporter = self.registry.exporter_for(PREVIEW_FORMAT)?;
        let made = self
            .project
            .working_snapshot(item)
            .map_err(Error::from)
            .and_then(|s| Ok(exporter.export(&s, PREVIEW_FORMAT, &item.dir)?));
        match made {
            Ok(p) => Some(p),
            Err(e) => {
                warnings.push(format!("превью не сделано: {e}"));
                None
            }
        }
    }
}

/// Поставить копию файла на место `dest` целиком: оборвавшееся копирование не
/// оставит половину модели, а файл можно заменить им же самим.
fn replace_file(file: &Path, dest: &Path) -> Result<()> {
    let tmp = dest.with_extension("tmp-import");
    let done = fs::copy(file, &tmp).and_then(|_| fs::rename(&tmp, dest));
    done.map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Error::io(dest, e)
    })
}
