//! Отпечаток условий перевода (`Converter::stamp`): от чего, кроме самого
//! файла, зависит STEP, который выгрузит SolidWorks. По нему и по
//! содержимому файла программа хранит готовый STEP на складе.
//!
//! В отпечаток входят:
//! - **установленный SolidWorks**: путь его `SLDWORKS.exe`, размер и время
//!   файла (обновление или пакет исправлений их меняют);
//! - **настройки STEP человека**: значения `STEP …` из
//!   `HKCU\Software\SolidWorks\SOLIDWORKS <год>\Export Settings`. Они только
//!   читаются: `SaveAs3` пишет STEP по настройкам человека, менять их нельзя;
//! - **активная конфигурация**: STEP пишется только с неё;
//! - **файлы, от которых зависит форма**: у сборки — файлы всех компонентов
//!   на любой глубине, у детали — документы внешних ссылок (эскиз «в
//!   контексте» сборки, заготовка производной детали). Список берётся из
//!   самого файла (`nonius-sldprt`, `Design`), у каждого файла — SHA-256
//!   содержимого.
//!
//! Где искать файл зависимости, знает только SolidWorks. Поэтому берутся
//! оба места, где он его найдёт вернее всего, — путь, записанный в файле, и
//! папка самого файла; что нашлось, то и входит в отпечаток. Не нашлось
//! нигде, файл не читается (старый формат) или манифест прочитан не целиком
//! — отпечатка нет (`None`): такой STEP на склад не кладётся, чтобы там не
//! оказался устаревший.

use std::path::{Path, PathBuf};

use nonius_sldprt::design::{DocKind, Piece};
use nonius_sldprt::Design;
use sha2::{Digest, Sha256};

/// Отпечаток условий перевода файла `source`.
pub(crate) fn stamp(source: &Path) -> Option<String> {
    stamp_with(&environment()?, source)
}

/// То же при известном окружении (SolidWorks и его настройки).
fn stamp_with(environment: &str, source: &Path) -> Option<String> {
    let mut lines = vec![environment.to_string()];
    lines.extend(depends(source)?);
    Some(lines.join("\n"))
}

/// Путь `SLDWORKS.exe` установленного SolidWorks — того, что записан в
/// реестре как `SldWorks.Application`.
pub(crate) fn exe() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CLASSES_ROOT;
        let root = winreg::RegKey::predef(HKEY_CLASSES_ROOT);
        let class: String = root
            .open_subkey(r"SldWorks.Application\CLSID")
            .and_then(|k| k.get_value(""))
            .ok()?;
        let server: String = root
            .open_subkey(format!(r"CLSID\{class}\LocalServer32"))
            .and_then(|k| k.get_value(""))
            .ok()?;
        exe_of(&server).filter(|p| p.is_file())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Путь программы из записи реестра: без кавычек и ключей после `.exe`.
#[cfg(any(windows, test))]
fn exe_of(server: &str) -> Option<PathBuf> {
    let server = server.trim().trim_start_matches('"');
    let end = server.to_lowercase().find(".exe")? + 4;
    Some(PathBuf::from(&server[..end]))
}

/// SolidWorks на этом компьютере и настройки STEP человека.
fn environment() -> Option<String> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        let exe = exe()?;
        let meta = std::fs::metadata(&exe).ok()?;
        let at = meta.modified().ok()?;
        let at = at.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
        let mut lines = vec![format!("solidworks {} {} {at}", exe.display(), meta.len())];
        // Настроек может не быть вовсе (SolidWorks ещё не запускали) —
        // тогда они заводские, и это тоже ответ.
        let user = winreg::RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(all) = user.open_subkey(r"Software\SolidWorks") {
            let mut years: Vec<String> = all.enum_keys().flatten().collect();
            years.retain(|y| y.starts_with("SOLIDWORKS 20"));
            years.sort();
            for year in years {
                let Ok(export) = all.open_subkey(format!(r"{year}\Export Settings")) else {
                    continue;
                };
                let mut values: Vec<String> = export
                    .enum_values()
                    .flatten()
                    .filter(|(name, _)| name.starts_with("STEP"))
                    .map(|(name, value)| format!("{year} {name}={value}"))
                    .collect();
                values.sort();
                lines.extend(values);
            }
        }
        Some(lines.join("\n"))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn digest(file: &Path) -> Option<String> {
    let mut hasher = Sha256::new();
    std::io::copy(&mut std::fs::File::open(file).ok()?, &mut hasher).ok()?;
    Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

/// Имя файла из пути Windows — и там, где `\` разделителем не считается.
fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// Строки отпечатка о самом документе: активная конфигурация и файлы, от
/// которых зависит его форма. `None` — узнать это не удалось.
fn depends(source: &Path) -> Option<Vec<String>> {
    let design = Design::open(source).ok()?;
    let whole = |piece| !design.gaps.iter().any(|g| g.piece == piece);
    if !whole(Piece::Catalog) || !whole(Piece::Manifest) {
        return None;
    }
    let manifest = design.manifest.as_ref()?;
    let active = manifest.active().map_or("", |c| c.name.as_str());
    let mut lines = vec![format!("configuration {active}")];

    let mut wanted: Vec<&str> = match design.kind? {
        DocKind::Assembly => {
            let parts = manifest.components.iter();
            let files = parts.filter(|c| !c.virtual_part && !c.imported);
            files.filter_map(|c| c.path.as_deref()).collect()
        }
        // Импорт (`MBimport`) не в счёт: его тело лежит в самой детали.
        DocKind::Part => {
            let external = manifest.external.iter();
            let live = external.filter(|e| e.kind != "MBimport");
            live.filter_map(|e| e.document.as_deref()).collect()
        }
        DocKind::Drawing => return None,
    };
    let own = manifest.documents.first().map_or("", |d| d.path.as_str());
    wanted.retain(|p| !p.eq_ignore_ascii_case(own) && !p.is_empty());
    wanted.sort_unstable();
    wanted.dedup();
    for path in wanted {
        let name = file_name(path);
        let recorded = PathBuf::from(path);
        let beside = source.parent().map(|dir| dir.join(name));
        let mut found = 0;
        for (place, file) in [("recorded", Some(recorded)), ("beside", beside)] {
            let Some(print) = file.filter(|f| f.is_file()).and_then(|f| digest(&f)) else {
                continue;
            };
            lines.push(format!("{place} {name} {print}"));
            found += 1;
        }
        if found == 0 {
            return None;
        }
    }
    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nonius_sldprt::Container;

    /// Манифест детали «вал», которая опирается на `заготовка.sldprt`.
    fn part(external: &str) -> Vec<u8> {
        let features = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<swSolidWorks xmlns="http://www.solidworks.com/sw2003/schema" swVersion="18000"><swHeader><swFile id="3" swDocType="PART" swPath="C:\нет такой папки\вал.SLDPRT"/><swFile id="5" swDocType="PART" swPath="C:\нет такой папки\заготовка.sldprt"/></swHeader><swModelList><swModel id="2" swName="вал" swFileRef="3"/><swModel id="6" swName="заготовка" swFileRef="5"/></swModelList><swConfigurationList><swConfiguration id="1" swName="По умолчанию" swID="0" swMostRecentConfiguration="YES" swConfigurationNeedsUpdate="NO" swModelRef="2"/></swConfigurationList><swExtFeatureList>{external}</swExtFeatureList></swSolidWorks>"#
        );
        Container::pack(&[
            ("swXmlContents/KeyWords", b"<Keywords id=\"1\" Name=\"x\"/>"),
            ("swXmlContents/Features", features.as_bytes()),
        ])
    }

    fn external(kind: &str) -> String {
        format!(
            r#"<swExtFeature id="4" swName="заготовка" swFeatureType="{kind}" swModelRef="6" swID="59"/>"#
        )
    }

    #[test]
    fn part_without_references_depends_on_itself_only() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("КТМ 640 вал.SLDPRT");
        std::fs::write(&file, part("")).unwrap();
        let stamp = stamp_with("SolidWorks 2025", &file).unwrap();
        assert!(stamp.starts_with("SolidWorks 2025\n"), "{stamp}");
        assert!(stamp.contains("configuration По умолчанию"), "{stamp}");
        assert_ne!(stamp_with("SolidWorks 2026", &file), Some(stamp));
        // Импортированное тело лежит в самой детали: исходник не нужен.
        std::fs::write(&file, part(&external("MBimport"))).unwrap();
        assert!(stamp_with("x", &file).is_some());
    }

    #[test]
    fn referenced_file_is_part_of_the_stamp() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("вал.SLDPRT");
        std::fs::write(&file, part(&external("Stock"))).unwrap();
        // Заготовки нет нигде: какой выйдет STEP, знает только SolidWorks.
        assert_eq!(stamp_with("x", &file), None);

        let stock = tmp.path().join("заготовка.sldprt");
        std::fs::write(&stock, "раз").unwrap();
        let first = stamp_with("x", &file).unwrap();
        assert!(first.contains("beside заготовка.sldprt "), "{first}");
        // Заготовку изменили — STEP детали уже другой.
        std::fs::write(&stock, "два").unwrap();
        let second = stamp_with("x", &file).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn unreadable_file_has_no_stamp() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("старая.sldprt");
        std::fs::write(&file, "не файл SolidWorks").unwrap();
        assert_eq!(stamp_with("x", &file), None);
        assert_eq!(stamp_with("x", &tmp.path().join("нет.sldprt")), None);
    }

    #[test]
    fn program_path_is_cut_out_of_the_registry_record() {
        let path = |s| exe_of(s).map(|p| p.to_string_lossy().into_owned());
        assert_eq!(
            path(r#""C:\Program Files\SW\SLDWORKS.exe" /Automation"#).as_deref(),
            Some(r"C:\Program Files\SW\SLDWORKS.exe")
        );
        assert_eq!(
            path(r"C:\PROGRA~1\SOLIDW~1\SLDWORKS.exe").as_deref(),
            Some(r"C:\PROGRA~1\SOLIDW~1\SLDWORKS.exe")
        );
        assert_eq!(path("что-то другое"), None);
    }
}
