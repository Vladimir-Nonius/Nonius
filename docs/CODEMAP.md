# Карта кода

Зачем: чтобы человек или ИИ-агент читал только нужные файлы, а не весь репозиторий. Найдите задачу в первой таблице, откройте указанные файлы. Остальное — по ссылкам из них.

Каждый файл начинается с `//!` — одной-двух строк о том, что в нём. Если не уверены, какой файл открыть, смотрите эти шапки: `grep -rn "^//!" crates/<крейт>/src | head -40`.

## Что читать для задачи

| Задача | Файлы |
|---|---|
| Новое действие программы (для CLI, окна, MCP) | `nonius-app/src/lib.rs`, файл темы в `nonius-app/src/` |
| Новая команда `nonius …` | действие в `nonius-app`, затем `nonius-cli/src/args.rs` (команда и справка), `main.rs` (вызов), печать в `commands/` |
| Изменить вывод команды | `nonius-cli/src/commands/<группа>.rs`, общие мелочи — `format.rs` |
| Импорт файла модели | `nonius-app/src/import.rs` |
| Материал, плотность, масса | `nonius-app/src/passport.rs`, справочник — `materials.rs` |
| Сравнение версий и его кэш | `nonius-app/src/compare.rs`, `cache.rs` |
| Статус деталей (новая / изменена / сохранена) | `nonius-app/src/items.rs` |
| Версии, фиксация, откат | `nonius-store/src/history.rs` |
| Файлы версии, кэш, снимки для модулей | `nonius-store/src/files.rs` |
| Формат сообщения коммита | `nonius-store/src/message.rs` |
| Новый модуль (крейт) | `nonius-core/src/extension/traits.rs`, `extension/manifest.rs`, `contract.rs`; пример — `nonius-step/src/lib.rs` |
| Поле в `meta.yaml` / `tree.yaml` / `project.yaml` | `nonius-core/src/meta.rs` / `tree.rs` / `project/mod.rs` + `schema.rs` (миграция) |
| Сравнение паспортов (числа, материал, свойства) | `nonius-core/src/compare/meta.rs`, формат отчёта — `compare/mod.rs` |
| Чтение заголовка STEP | `nonius-step/src/header.rs` |
| Геометрия OpenCascade (объём, габариты) | `nonius-occt/src/modules/source.rs`, `shape.rs` |
| Сравнение формы | `nonius-occt/src/modules/diff.rs`, `modules/compare.rs` |
| Картинка изменений, цвета | `nonius-occt/src/modules/highlight.rs`, `render/mod.rs`; сама отрисовка — `render/raster.rs` |
| Новая функция OpenCascade | `nonius-occt/cpp/nonius_occt.h` и `.cpp`, `src/ffi.rs`, `src/shape.rs` |
| Сборка с OpenCascade / без него | `nonius-occt/build.rs`, `nonius-occt/src/lib.rs` |
| CI и сборка nonius.exe | `.github/workflows/ci.yml`, `release.yml`, `packaging/` |

## Крейты и файлы

Пути — от `crates/`.

### `nonius-core` — ядро

| Файл | Что внутри |
|---|---|
| `lib.rs` | список модулей ядра и короткие пути (`PartName`, `Project`, …) |
| `names.rs` | имя детали (`PartName`) и номер версии (`VersionNumber`) |
| `schema.rs` | `schema: N` в файлах данных, чтение, запись, миграции |
| `meta.rs` | `meta.yaml` — паспорт версии: геометрия, материал, свойства |
| `tree.rs` | `tree.yaml` — дерево построения |
| `project/mod.rs` | папка проекта, `project.yaml`, детали и сборки |
| `project/templates.rs` | `.gitattributes` и `AGENTS.md` нового проекта |
| `extension/manifest.rs` | манифест модуля, точки расширения, права, версия API |
| `extension/traits.rs` | трейты точек (`GeometrySource`, `Comparator`, …), `Snapshot`, `Event` |
| `extension/registry.rs` | реестр модулей и проверка при подключении |
| `contract.rs` | контрактные проверки для каждой точки |
| `compare/mod.rs` | формат отчёта: `Change`, `ChangeKind`, `ChangeReport` |
| `compare/meta.rs` | встроенное сравнение паспортов `meta-compare` |
| `compare/number.rs` | запись чисел без лишних нулей |
| `error.rs` | ошибки ядра |

### `nonius-store` — версии на git + LFS

| Файл | Что внутри |
|---|---|
| `lib.rs` | `Store`: открытие, первый коммит, поиск детали |
| `history.rs` | список версий, изменения, `commit`, `restore` |
| `files.rs` | файлы версии, `export`, кэш версий, `snapshot` |
| `message.rs` | сообщение коммита и строки `Nonius-Item` / `Nonius-Version` |
| `version.rs` | `VersionInfo`, `FileChange` |
| `git.rs` | запуск системного `git` |
| `workdir.rs` | служебная папка `.git/nonius`, запись в неё целыми папками (временная папка → переименование) |
| `error.rs` | ошибки хранилища |

### `nonius-step` — заголовок STEP

| Файл | Что внутри |
|---|---|
| `lib.rs` | модуль `step-header` (`StepHeaderSource`) и его тесты |
| `header.rs` | разбор `FILE_NAME`, `FILE_SCHEMA`, кириллицы `\X2\` |

### `nonius-occt` — геометрия на OpenCascade

Без OpenCascade собирается без геометрии: `modules/`, `shape.rs`, `ffi.rs` компилируются только при `cfg(nonius_occt)`.

| Файл | Что внутри |
|---|---|
| `build.rs` | поиск OpenCascade, сборка C++-прослойки |
| `cpp/nonius_occt.h`, `.cpp` | C-прослойка над OpenCascade |
| `src/lib.rs` | `register`, `available`, `occt_version`, `preview_png`, запись PNG |
| `src/ffi.rs` | объявления функций прослойки |
| `src/shape.rs` | безопасная обёртка `Shape`: чтение, свойства, булевы операции, сетка |
| `src/modules/mod.rs` | общие помощники модулей, `preview` |
| `src/modules/source.rs` | `step-occt`: контрольные числа из STEP |
| `src/modules/diff.rs` | `GeometryDiff`: добавленный и убранный материал |
| `src/modules/compare.rs` | `geometry-occt`: сравнение формы |
| `src/modules/highlight.rs` | `highlight-occt`: `changes.png` |
| `src/modules/preview.rs` | `preview-occt`: экспорт `preview` — превью `preview.png` |
| `src/render/mod.rs` | сетка, слои, цвета, вид, картинка |
| `src/render/raster.rs` | отрисовка: камера, z-буфер, свет, «рентген» |

### `nonius-app` — действия программы

Общие для командной строки, окна и MCP. Ничего не печатает, возвращает данные. Модули не знает: реестр передаёт вызывающий.

| Файл | Что внутри |
|---|---|
| `lib.rs` | `App` (проект + реестр + хранилище), `create_project` |
| `items.rs` | новая деталь, список, `ItemStatus` и `ItemState` |
| `history.rs` | версии, `save_version`, `restore`, `export` |
| `import.rs` | `import`: файл модели в деталь, паспорт, превью |
| `passport.rs` | `Passport`, `Mass` (масса только при заданном материале), `set_material` |
| `materials.rs` | справочник материалов с плотностью |
| `compare.rs` | `compare`, `Comparison`, `CompareOptions`, `Progress` (ход и отмена) |
| `cache.rs` | кэш сравнений `.git/nonius/cache/<ключ>/cache.yaml` (`schema: 1`) |
| `error.rs` | ошибки действий |

### `nonius-cli` — команда `nonius`

Тонкая обёртка над `nonius-app`: разбор команды и печать.

| Файл | Что внутри |
|---|---|
| `main.rs` | запуск, выбор команды, набор модулей (`registry`) |
| `args.rs` | команды и параметры, текст справки |
| `welcome.rs` | подсказка при двойном щелчке |
| `format.rs` | значки изменений, номер версии, дата |
| `commands/project.rs` | `init`, `part new`, `list`, `info`, `modules` |
| `commands/import.rs` | `import` |
| `commands/history.rs` | `status`, `commit`, `log`, `restore`, `export` |
| `commands/compare.rs` | `diff`, `render` |
| `commands/passport.rs` | `material`, `materials` |

## Тесты

| Где | Что проверяют |
|---|---|
| `#[cfg(test)] mod tests` в файле | мелкие функции этого файла |
| `nonius-core/tests/project.rs` | папка проекта целиком |
| `nonius-store/tests/store.rs` | версии на настоящем git |
| `nonius-occt/tests/occt.rs`, `without_occt.rs` | геометрия с OpenCascade и сборка без него |
| `nonius-app/tests/app.rs` | действия на временном проекте: статус, история, масса, сравнение и кэш, отмена, два потока |
| `nonius-cli/tests/cli.rs` | команды `nonius` от начала до конца |

Карту обновляет тот, кто добавляет, переносит или удаляет файл, — в том же pull request.
