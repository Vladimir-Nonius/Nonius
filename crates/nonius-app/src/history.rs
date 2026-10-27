//! История детали: версии, сохранить версию, откатить, выгрузить.

use std::path::{Path, PathBuf};

use nonius_core::extension::Event;
use nonius_core::VersionNumber;
use nonius_store::VersionInfo;

use crate::{App, Result};

impl App {
    /// Версии детали, новые первыми.
    pub fn versions(&self, name: &str) -> Result<Vec<VersionInfo>> {
        let item = self.item(name)?;
        Ok(self.store()?.versions(&item)?)
    }

    /// Одна версия детали.
    pub fn version(&self, name: &str, version: VersionNumber) -> Result<VersionInfo> {
        let item = self.item(name)?;
        Ok(self.store()?.version(&item, version)?)
    }

    /// Сохранить текущее состояние детали новой версией. `message` — что и
    /// почему изменено; первая строка — короткое описание.
    pub fn save_version(&self, name: &str, message: &str) -> Result<VersionInfo> {
        let item = self.item(name)?;
        let v = self.store()?.commit(&item, message)?;
        self.notify(&v);
        Ok(v)
    }

    /// Вернуть деталь к старой версии. Создаётся новая версия с содержимым
    /// старой, история не переписывается.
    pub fn restore(
        &self,
        name: &str,
        version: VersionNumber,
        note: Option<&str>,
    ) -> Result<VersionInfo> {
        let item = self.item(name)?;
        let v = self.store()?.restore(&item, version, note)?;
        self.notify(&v);
        Ok(v)
    }

    /// Выгрузить все файлы версии в папку `dest`.
    pub fn export(
        &self,
        name: &str,
        version: VersionNumber,
        dest: &Path,
    ) -> Result<(VersionInfo, Vec<PathBuf>)> {
        let item = self.item(name)?;
        let store = self.store()?;
        let v = store.version(&item, version)?;
        let files = store.export(&item, &v, dest)?;
        Ok((v, files))
    }

    /// Сообщить модулям о новой версии. Ошибка обработчика не отменяет версию.
    fn notify(&self, v: &VersionInfo) {
        let _ = self.registry.emit(&Event::VersionCommitted {
            item: v.item.clone(),
            version: v.version,
        });
    }
}
