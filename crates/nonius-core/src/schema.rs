//! Файлы со схемой версии.
//!
//! Каждый файл данных Nonius (`project.yaml`, `meta.yaml`, `tree.yaml`, …)
//! начинается с поля `schema: N`. Программа читает файлы своей и более старых
//! схем (старые поднимаются миграциями) и отказывается читать файлы более новой
//! схемы, чтобы не испортить их.

use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_yaml::Value;

use crate::error::{Error, Result};

/// Тип файла данных со схемой версии.
pub trait Versioned: Serialize + DeserializeOwned {
    /// Вид файла для сообщений об ошибках: `project`, `meta`, `tree`.
    const KIND: &'static str;
    /// Текущая версия схемы, которую пишет эта программа.
    const CURRENT: u32;

    /// Поднять документ схемы `from` до схемы `from + 1`.
    ///
    /// По умолчанию миграций нет. Когда схема меняется, тип переопределяет
    /// этот метод и обрабатывает каждую старую версию.
    fn migrate(doc: Value, from: u32) -> std::result::Result<Value, String> {
        let _ = doc;
        Err(format!("нет миграции со схемы {from}"))
    }
}

/// Прочитать файл данных, проверить схему и при необходимости поднять её.
pub fn load<T: Versioned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    from_str(&text, path)
}

/// Разобрать текст файла данных. `path` нужен только для сообщений об ошибках.
pub fn from_str<T: Versioned>(text: &str, path: &Path) -> Result<T> {
    let yaml_err = |source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    };
    let mut doc: Value = serde_yaml::from_str(text).map_err(yaml_err)?;

    let found = doc
        .get("schema")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(0);
    if found == 0 || found > T::CURRENT {
        return Err(Error::UnsupportedSchema {
            path: path.to_path_buf(),
            kind: T::KIND,
            found,
            supported: T::CURRENT,
        });
    }

    for from in found..T::CURRENT {
        doc = T::migrate(doc, from).map_err(|message| Error::Module {
            module: format!("schema:{}", T::KIND),
            message: format!("{}: {message}", path.display()),
        })?;
        if let Value::Mapping(map) = &mut doc {
            map.insert(
                Value::String("schema".into()),
                Value::Number(serde_yaml::Number::from(from + 1)),
            );
        }
    }

    serde_yaml::from_value(doc).map_err(yaml_err)
}

/// Записать файл данных. Порядок полей стабильный, чтобы diff был чистым.
pub fn save<T: Versioned>(value: &T, path: &Path) -> Result<()> {
    let text = to_string(value, path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    fs::write(path, text).map_err(|e| Error::io(path, e))
}

/// Превратить файл данных в текст.
pub fn to_string<T: Versioned>(value: &T, path: &Path) -> Result<String> {
    serde_yaml::to_string(value).map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })
}
