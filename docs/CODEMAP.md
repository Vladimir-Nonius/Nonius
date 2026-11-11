# Карта кода

Зачем: чтобы человек или ИИ-агент читал только нужные файлы, а не весь репозиторий. Найдите задачу в первой таблице, откройте указанные файлы. Остальное — по ссылкам из них.

Каждый файл начинается с `//!` — одной-двух строк о том, что в нём. Если не уверены, какой файл открыть, смотрите эти шапки: `grep -rn "^//!" crates/<крейт>/src | head -40`.

## Что читать для задачи

| Задача | Файлы |
|---|---|
| Новое действие программы (для CLI, окна, MCP) | `nonius-app/src/lib.rs`, файл темы в `nonius-app/src/` |
| Новая команда `nonius …` | действие в `nonius-app`, затем `nonius-cli/src/args.rs` (команда и справка), `main.rs` (вызов), печать в `commands/` |
| Изменить вывод команды | `nonius-cli/src/commands/<группа>.rs`, общие мелочи — `format.rs`; ответ `--json` — `commands/json/` |
| Новый код ошибки | `code()` в `error.rs` своего крейта и список `ERROR_CODES` в `nonius-app/src/error.rs` |
| Задумано, но не сделано: сравнение в отдельном процессе | раздел «Задумано» в `docs/ARCHITECTURE.md`; идеи по интерфейсу — `docs/proposals/interface-next.md` |
| Импорт файла модели | `nonius-app/src/import.rs` |
| Разбор папки (Nonius сам раскладывает модели, версии и чертежи) | правила — `nonius-app/src/intake/plan.rs`, `grouping.rs`, `names.rs`; выполнение — `apply.rs`; связи файлов от SolidWorks — `nonius-solidworks/src/links.rs`, `links.ps1`; точка `LinkSource` — `nonius-core/src/extension/traits.rs`; команда — `nonius-cli/src/commands/intake.rs` |
| Файл SolidWorks без ручного STEP (Nonius сам просит STEP у SolidWorks) | разговор с SolidWorks — `nonius-solidworks/src/export_step.ps1`, запуск — `run.rs`; где импорт это зовёт — `nonius-app/src/import.rs` (`convert`); точка `Converter` — `nonius-core/src/extension/traits.rs` |
| Материал, плотность, масса | `nonius-app/src/passport.rs`, справочник — `materials.rs` |
| Сравнение версий и его кэш | `nonius-app/src/compare.rs`, `cache.rs` |
| Статус деталей (новая / изменена / сохранена) | `nonius-app/src/items.rs` |
| Версии, фиксация, откат | `nonius-store/src/history.rs` |
| Удалить, вернуть, переименовать деталь, убрать версию с карты | `nonius-store/src/items.rs`, чьи версии — `lineage.rs`, отметки в истории — `message.rs` |
| Папки проекта: где детали, папки-группы, перенос детали, обновление старого проекта | обход — `nonius-core/src/project/layout.rs`; перенос и `upgrade` — `nonius-store/src/layout.rs`; действия и карта — `nonius-app/src/layout.rs`; команды `map`, `folder new`, `part move`, `upgrade` — `nonius-cli/src/commands/layout.rs`; окошко и полоса — `nonius-gui/src/app/upgrade.rs`; дерево с папками и перетаскивание в окне — `app/tree_folders.rs`, окошко «Новая папка» — `app/new_folder.rs`, перенос — `app/command.rs` → очередь `app/batch.rs` |
| Сравнить с файлом или другой деталью | `nonius-app/src/outside.rs`; в окне — `compare_with` в `nonius-gui/src/app/part_actions.rs` |
| Файлы версии, кэш, снимки для модулей | `nonius-store/src/files.rs` |
| Разница файлов между версиями, открыть файл | `nonius-store/src/diff.rs`, `nonius-app/src/files.rs` |
| Свой файл детали: добавить, убрать, прочитать текст | `nonius-app/src/files.rs`; в окне — `add_files` в `nonius-gui/src/app/part_actions.rs`, вкладка `app/files.rs` |
| Программы для файла, запуск, показ в папке | `nonius-app/src/system/` (`windows.rs`, запасной `other.rs`) |
| Меню правого щелчка | `nonius-gui/src/app/open_menu.rs` |
| Сервер MCP | `nonius-cli/src/commands/mcp.rs`; новый инструмент — команда в `commands/json/mod.rs` (`COMMANDS`) |
| Правила для ИИ в проекте | `nonius-core/src/project/agents.rs` |
| Формат сообщения коммита, подпись ИИ | `nonius-store/src/message.rs`, `version.rs` (`AiActor`) |
| Статус всего проекта одним проходом | `nonius-store/src/overview.rs`, `nonius-app/src/items.rs` |
| Новый модуль (крейт) | `nonius-core/src/extension/traits.rs`, `extension/manifest.rs`, `contract.rs`; пример — `nonius-step/src/lib.rs` |
| Поле в `meta.yaml` / `tree.yaml` / `project.yaml` | `nonius-core/src/meta.rs` / `tree.rs` / `project/mod.rs` + `schema.rs` (миграция) |
| Сравнение паспортов (числа, материал, свойства) | `nonius-core/src/compare/meta.rs`, формат отчёта — `compare/mod.rs` |
| Чтение заголовка STEP | `nonius-step/src/header.rs` |
| Геометрия OpenCascade (объём, габариты) | `nonius-occt/src/modules/source.rs`, `shape.rs` |
| Сравнение формы (и что считается шумом) | `nonius-occt/src/modules/diff.rs`, `modules/compare.rs`; скорость — `cpp/local.cpp` (только изменённые места), `cpp/share.cpp`, `src/cache.rs` |
| Изменения словами (отверстия, скругления, плоскости) | `nonius-occt/src/changes/` (слова, без OpenCascade), `cpp/faces.cpp`, `src/faces.rs`, `modules/features.rs` |
| Какие грани версий совпали | `nonius-occt/cpp/match.cpp`, `cpp/same.cpp` |
| Замер сравнения на своей детали | `nonius-occt/examples/probe.rs`; шаги C++ — переменная `NONIUS_TRACE=1` |
| Сцена для окна (места изменений, glTF) | `nonius-scene/src/lib.rs` (модель), `glb/write.rs`, `glb/read.rs`, `contract.rs`; из OpenCascade — `nonius-occt/src/modules/scene.rs` |
| Окно: панели, надписи, выбор версий | `nonius-gui/src/app/mod.rs` (порядок работы), `app/actions.rs` и `results.rs` (действия и их итог), `app/panels.rs`, `app/legend.rs` (цвета и места), `app/toolbar.rs`, надписи — `texts.rs` |
| Окно: меню «Файл», «Деталь», «Вид» | `nonius-gui/src/app/file_menu.rs`; окна выбора файла — `app/choose.rs` |
| Окно: новая деталь, добавление модели | `nonius-gui/src/app/add_model/` (очередь файлов — `mod.rs`, окошко — `dialog.rs`), `app/model_names.rs` (угадать деталь, где она лежит), `app/dropped.rs` (перетаскивание), `app/new_part.rs`, `app/start.rs` |
| Окно: удалить, переименовать, вернуть, выгрузить | `nonius-gui/src/app/part_actions.rs` (действия), `app/part_dialogs.rs` (окошки); несколько сразу — `app/command.rs` и очередь `app/batch.rs` |
| Окно: первый запуск, git и имя автора | `nonius-gui/src/app/first_run.rs`, `nonius-store/src/machine.rs` |
| Один проект в одном окне | `nonius-cli/src/window_lock.rs` |
| Окно: вкладки проектов | `nonius-gui/src/app/strip.rs` (полоса, переключение, память), состояние вкладки — `app/project_tab.rs` |
| Окно: черновик без папки, «Новый проект», «Сохранить проект как…» | `nonius-gui/src/app/draft.rs`; перенос проекта — `nonius-app/src/moving.rs`; Корзина и замыкания `mover`, `discard` — `nonius-cli/src/window.rs` |
| Окно: дерево деталей и папок, карта версий | `nonius-gui/src/app/tree.rs`, `app/tree_folders.rs`, `app/map.rs` |
| Окно: история, сохранение версии, материал и масса | `nonius-gui/src/app/history.rs`, `save.rs`, `material.rs`; данные — `source/` и `nonius-cli/src/window/source/` |
| Окно: вкладки под 3D | `nonius-gui/src/app/bottom.rs` (панель, «Числа»), `app/files.rs` (`«Файлы»`) |
| 3D-вид: мышь, камера, отрисовка | `nonius-gui/src/view/controls.rs` (мышь и клавиши), `view/camera.rs`, `view/model.rs` (режимы и цвета), `view/gpu/mod.rs` |
| Откуда окно берёт проект | `nonius-gui/src/source/mod.rs` (трейт), `source/entries.rs` (что приходит: деталь, версия, файл…), `nonius-cli/src/window/source/` (реализация на `nonius-app`) |
| Сценарии человека на настоящем проекте | `nonius-cli/src/window/scenarios.rs` |
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
| `project/mod.rs` | папка проекта, `project.yaml` (схема 2), создание деталей, старый формат |
| `project/layout.rs` | обход папок: детали (папки с `meta.yaml`) и папки-группы, создание папки |
| `project/templates.rs` | `.gitattributes` нового проекта |
| `project/agents.rs` | `AGENTS.md` проекта: правила для ИИ, обновление блока между метками |
| `extension/manifest.rs` | манифест модуля, точки расширения, права, версия API |
| `extension/traits.rs` | трейты точек (`GeometrySource`, `Comparator`, `Converter`, …), `Snapshot`, `Event` |
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
| `items.rs` | удалить, вернуть, переименовать деталь |
| `lineage.rs` | чья какая версия и в какой папке она лежала, список удалённых |
| `layout.rs` | перенос детали в папку, перевод старого проекта на папки |
| `files.rs` | файлы версии, `export`, кэш версий, `snapshot` |
| `diff.rs` | разница между версиями: файлы, размеры, строки текста |
| `overview.rs` | история и изменения всех деталей за один проход |
| `message.rs` | сообщение коммита, строки `Nonius-Item` / `Nonius-Version` / `Nonius-AI` |
| `version.rs` | `VersionInfo`, `AiActor`, `FileChange` |
| `git.rs` | запуск системного `git` |
| `machine.rs` | что есть на компьютере: git, LFS, имя автора; запомнить имя |
| `workdir.rs` | служебная папка `.git/nonius`, запись в неё целыми папками (временная папка → переименование) |
| `error.rs` | ошибки хранилища |

### `nonius-step` — заголовок STEP

| Файл | Что внутри |
|---|---|
| `lib.rs` | модуль `step-header` (`StepHeaderSource`) и его тесты |
| `header.rs` | разбор `FILE_NAME`, `FILE_SCHEMA`, кириллицы `\X2\` |

### `nonius-solidworks` — STEP из файлов SolidWorks

Только Windows и только если SolidWorks установлен; иначе модуль не подключается, и импорт работает без него.

| Файл | Что внутри |
|---|---|
| `lib.rs` | модуль `solidworks-step` (`SolidWorksConverter`, точка `Converter`), `register` — подключить, если SolidWorks есть |
| `run.rs` | `available` (есть ли SolidWorks), `export_step`, `script`: запуск скрипта без окна, ожидание до 5 минут, разбор ответа `OK` / `ERROR: причина`; `close_later` — оставить сторожа |
| `links.rs`, `links.ps1` | модуль `solidworks-links` (точка `LinkSource`): из чего собрана сборка, к какой модели чертёж; вся папка одним запуском |
| `close.ps1` | сторож: держит SolidWorks запущенным, пока Nonius к нему обращается, и закрывает наш невидимый после полуминуты простоя (иначе разбор папки запускал бы его на каждую деталь) |
| `export_step.ps1` | сам разговор с SolidWorks (PowerShell + COM): открыть деталь, «сохранить как STEP», закрыть за собой; в начале — правила вежливости к работающему человеку и почему вызовы идут через `InvokeMember` |

### `nonius-occt` — геометрия на OpenCascade

Без OpenCascade собирается без геометрии: `modules/`, `shape.rs`, `ffi.rs` компилируются только при `cfg(nonius_occt)`.

| Файл | Что внутри |
|---|---|
| `build.rs` | поиск OpenCascade, сборка C++-прослойки |
| `cpp/nonius_occt.h`, `.cpp` | C-прослойка над OpenCascade: STEP, BRep, булевы операции, разность версий, сетка |
| `cpp/props.cpp` | контрольные числа: объём, площадь, центр масс, габариты (грани параллельно) |
| `cpp/share.h`, `.cpp` | общие грани двух версий — сравнение пересекает только изменённое |
| `cpp/same.h`, `.cpp` | совпадение рёбер и граней двух версий |
| `cpp/internal.h` | общее для файлов прослойки |
| `src/lib.rs` | `register`, `available`, `occt_version`, `preview_png`, запись PNG |
| `src/ffi.rs` | объявления функций прослойки |
| `src/shape.rs` | безопасная обёртка `Shape`: чтение, свойства, булевы операции, сетка |
| `src/cache.rs` | кэш прочитанных STEP (BRep в папке временных файлов) |
| `src/modules/mod.rs` | общие помощники модулей, `preview` |
| `src/modules/source.rs` | `step-occt`: контрольные числа из STEP |
| `src/modules/diff.rs` | `GeometryDiff`: добавленный и убранный материал по кускам (`Piece`), шум по толщине куска, проверка баланса объёмов, пометки «приблизительно» и «нет твёрдых тел»; `shared_diff`: одна булева операция на сравнение и сцену |
| `src/modules/compare.rs` | `geometry-occt`: сравнение формы |
| `src/modules/features.rs` | `features-occt`: что сделали с деталью — словами |
| `src/changes/` | слова из изменённых граней: `round.rs` — отверстия, бобышки, скругления; `flat.rs` — плоскости; собирается и без OpenCascade |
| `src/faces.rs` | `Shape::changed_faces`: грани, которые есть только в одной версии |
| `cpp/match.h`, `.cpp` | сопоставление рёбер, вершин и граней двух версий |
| `cpp/faces.cpp` | описание изменённых граней числами: вид поверхности, ось, радиус |
| `cpp/local.h`, `.cpp` | разница версий только там, где они отличаются |
| `src/modules/highlight.rs` | `highlight-occt`: `changes.png` |
| `src/modules/scene.rs` | `scene-occt`: сцена `changes.glb` для окна, каждый кусок изменений — отдельное место |
| `src/render/mod.rs` | сетка, слои, цвета, вид, картинка |
| `src/render/raster.rs` | отрисовка: камера, z-буфер, свет, «рентген» |

### `nonius-scene` — модель сцены и glTF

| Файл | Что внутри |
|---|---|
| `lib.rs` | `Scene`, `Region` (место), `Role`, `Mesh`, `Bounds`, `FORMAT_VERSION` |
| `glb/mod.rs` | ошибки чтения, единицы, номер формата |
| `glb/write.rs` | сцена → `.glb` (старая версия — во второй сцене glTF) |
| `glb/read.rs` | `.glb` → сцена, через библиотеку `gltf` |
| `contract.rs` | контрактная проверка сцены и файла `.glb` |

### `nonius-gui` — окно программы

| Файл | Что внутри |
|---|---|
| `lib.rs` | `run` — открыть окно, `show_error` — сообщение без консоли |
| `source/mod.rs` | `ProjectSource`: детали, папки, версии, сравнение, сохранение, материал и масса — всё, что окно берёт у программы; `Progress` (ход и отмена) |
| `source/entries.rs` | что приходит в окно: `PartEntry` (с папкой), `Listing` (детали и папки), версии, файлы, массы |
| `texts.rs` | все надписи окна (русский) |
| `app/mod.rs` | `App`: состояние окна, раскладка панелей (`draw`); `raw_input_hook` забирает стрелки ↑/↓ у открытого меню |
| `app/actions.rs` | действия: открыть проект, выбрать деталь и версии, сравнить, отменить, сохранить, назначить материал |
| `app/results.rs` | что делать с готовыми результатами фоновой работы |
| `app/file_menu.rs` | меню «Файл», «Деталь», «Вид»; какие файлы считаются моделями, поиск моделей в папке |
| `app/choose.rs` | системные окна выбора файла и папки (`Chooser`) |
| `app/part_actions.rs` | действия над деталью: переименовать, удалить, вернуть, выгрузить, сравнить с файлом |
| `app/part_dialogs.rs` | окошки этих действий, «Удалить выделенное (N)?» и список удалённых деталей |
| `app/add_model/` | «Добавить модель» на каждый файл: очередь, сразу сохраняет версию, новая деталь — в папку, куда бросили (`mod.rs`); само окошко (`dialog.rs`) |
| `app/model_names.rs` | что за деталь в файле: то же имя или то же без хвоста версии |
| `app/dropped.rs` | перетаскивание: модели, папка с моделями, папка проекта |
| `app/first_run.rs` | окошко первого запуска: имя автора, нет git |
| `app/new_part.rs` | окошко «Новая деталь» (в выбранную папку) |
| `app/new_folder.rs` | окошко «Новая папка»; итог переноса детали (`part_moved`), раскрыть папку до детали (`unfold_to`) |
| `app/tree.rs` | дерево слева: поиск, строки деталей, внутри раскрытых — история |
| `app/structure/` | «Деталь (3D) / Структура»: папка — блок в рамке со счётчиком, заголовок выделяет папку, пустое место снимает выделение (`mod.rs`); карточка — одно целое: щелчок, правая кнопка и перетаскивание за любое место; полоска вида и файлы папки (`cards.rs`) |
| `app/drag_hint.rs` | пока несут мышью: табличка у курсора («Переместить в …», «Скопировать в …», «Сюда нельзя»), заливка всей папки-цели, раскрытие свёрнутой папки; отпустили деталь — перенос туда, что подсвечено |
| `app/drop_zones.rs` | файл из Проводника: куда бросили (курсор + места строк из `shown.rs`); на папку — модель новой деталью в неё, остальное копией; на деталь — версия или `docs/`; открыть файл папки |
| `app/node.rs` | `Node`: что это на экране — деталь, папка или файл папки; куда ляжет брошенное на него |
| `app/shown.rs` | что где на экране в этом кадре (видимый прямоугольник → `Node`), предмет под курсором: самый мелкий; всё место папки; порядок предметов в виде для Shift |
| `app/selection.rs` | выделение, общее для дерева и «Структуры»: щелчок, Ctrl, Shift (`Selection::click`), `App::pick` — что значит щелчок по предмету |
| `app/command.rs` | одна команда для клавиш, меню и перетаскивания (`Command`, `App::command`); клавиши выделенного: Delete, Ctrl+A, Esc, F2, Enter |
| `app/choice.rs` | выбор с клавиатуры: в окошке главная кнопка сразу в фокусе, ←/→ — на соседнюю; в меню ↑/↓ по пунктам по кругу; Enter нажимает |
| `app/batch.rs` | очередь: перенести или удалить несколько одной фоновой работой, «Остановить», итог «Перенесено 3 из 5» |
| `app/marquee.rs` | рамка мышью с пустого места дерева и «Структуры»: что задела, то выделено (с Ctrl — добавлено) |
| `app/settings_window.rs` | окно «Настройки»: «Проект» (название, допуск — `project.yaml`), «Программа» (вид на этом компьютере) |
| `app/tree_folders.rs` | папки в дереве: строки папок и файлов, кнопки «+ деталь», «+ папка», «свернуть всё», перетаскивание детали на папку или в корень; исполнение щелчков, порядок строк для Shift и Ctrl+A |
| `app/map.rs` | карта версий снизу: кружки на линии, щелчок и правая кнопка как в истории |
| `app/panels.rs` | полоса ошибки, «Что изменилось» справа, строка состояния с «Отменить» и «Остановить» (очередь), пустые состояния и надписи поверх 3D |
| `app/legend.rs` | цвета с объёмами, список мест, мелкие места одной строкой |
| `app/history.rs` | история внутри дерева: «Сейчас», версии, метки «было» и «стало», «Сохранить версию»; меню версии (открыть модель копией, «Было»/«Стало», вернуть, выгрузить) и действия, общие с картой |
| `app/save.rs` | окошко «Сохранить версию»: что изменилось и описание |
| `app/bottom.rs` | вкладки под 3D, таблица «Числа» |
| `app/project_tab.rs` | вкладка проекта: всё, что окно знает об одном проекте, своя очередь работ |
| `app/strip.rs` | полоса вкладок проектов: «+», крестик, переключение, память между запусками |
| `app/upgrade.rs` | проект старого формата: окошко «Обновить?», жёлтая полоса, обновление; открыть проект заново после настроек |
| `app/draft.rs` | «Новый проект»: черновик во временной папке, жёлтая полоса, перенос, вопрос при закрытии, «Не сохранять» — в Корзину |
| `app/files.rs` | вкладка «Файлы»: список, разница текста, «Открыть» |
| `app/open_menu.rs` | меню правого щелчка: «Открыть», «Открыть в…», «Показать в папке» (`open_items` — общие и для меню версии), «Добавить версию…», «Переименовать…», «Удалить…»; щелчок по одному из нескольких выделенных — меню выделенного (`attach_node`: «Удалить выделенное (N)…») |
| `app/material.rs` | «Материал и масса»: справочник, свой материал, масса «было → стало» |
| `app/toolbar.rs` | «Было» / «Стало», режимы, кнопки видов, сохранение картинки |
| `app/start.rs` | начальный экран: открыть или создать проект |
| `app/jobs.rs` | фоновая работа, ход выполнения, отмена сравнения, живые потоки (перед переносом черновика) |
| `app/settings.rs` | что окно помнит между запусками (`window.txt`) |
| `app/numbers.rs` | числа для человека: разряды, округление, масса, плотность |
| `view/widget.rs` | 3D-вид в окне: ввод, кадр, оси в углу |
| `view/controls.rs` | мышь и клавиши как в SolidWorks → движения камеры |
| `view/camera.rs` | ортогональная камера, виды, вращение, масштаб к курсору |
| `view/model.rs` | режимы «Было / Изменения / Стало», цвета, выбранное место |
| `view/mesh.rs` | нормали и рёбра сетки |
| `view/pick.rs` | что под курсором |
| `view/gpu/mod.rs` | отрисовка wgpu: порядок кусков, рентген, рёбра |
| `view/gpu/pipelines.rs`, `shader.wgsl` | конвейеры и шейдер |
| `view/gpu/target.rs`, `readback.rs` | картинка кадра и чтение её в PNG |
| `src/modules/preview.rs` | `preview-occt`: экспорт `preview` — превью `preview.png` |
| `src/render/mod.rs` | сетка, слои, цвета, вид, картинка |
| `src/render/raster.rs` | отрисовка: камера, z-буфер, свет, «рентген» |

### `nonius-app` — действия программы

Общие для командной строки, окна и MCP. Ничего не печатает, возвращает данные. Модули не знает: реестр передаёт вызывающий.

| Файл | Что внутри |
|---|---|
| `lib.rs` | `App` (проект + реестр + хранилище), `create_project` |
| `items.rs` | новая деталь, список, `ItemStatus` и `ItemState` |
| `layout.rs` | карта проекта, папки, перенос детали, обновление старого проекта |
| `settings.rs` | настройки проекта: название и допуск сравнения, запись одной записью истории |
| `history.rs` | версии, `save_version`, `restore`, `export` |
| `files.rs` | файлы версии, разница, копия и открытие файла |
| `system/` | программы для типа файла, запуск, «Открыть с помощью», показ в папке; Windows только в `windows.rs` |
| `intake/mod.rs` | разбор папки: данные плана (`IntakePlan`), обход папки, `intake_plan` |
| `intake/names.rs` | пометки версий в именах файлов: `v2`, `(2)`, «копия», «доработанный» |
| `intake/grouping.rs` | какие модели — версии одной детали; имена деталей без повторов |
| `intake/plan.rs` | правила: модель + её чертежи и бумаги, папки по сборкам, сам план |
| `intake/apply.rs` | `intake_apply`: копии файлов в проект, версии в историю, итог |
| `import.rs` | `import`: файл модели в деталь, паспорт, превью; для файла SolidWorks — свежий STEP через модуль перевода (`convert`) |
| `passport.rs` | `Passport`, `Mass` (масса только при заданном материале), `set_material` |
| `materials.rs` | справочник материалов с плотностью |
| `compare.rs` | `compare`, `Comparison`, `CompareOptions`, `Progress` (ход и отмена) |
| `model_view.rs` | `model_view`: деталь как есть, без сравнения и версий |
| `cache.rs` | кэш сравнений `.git/nonius/cache/<ключ>/cache.yaml` (`schema: 1`) |
| `error.rs` | ошибки действий |

### `nonius-cli` — команда `nonius`

Тонкая обёртка над `nonius-app`: разбор команды и печать.

| Файл | Что внутри |
|---|---|
| `main.rs` | запуск, выбор команды, набор модулей (`registry`) |
| `args.rs` | команды и параметры, текст справки |
| `window.rs` | окно: `nonius` без команды, `nonius view` |
| `window/source/mod.rs` | мост от `nonius-app` к окну (`AppSource`), замки чтения и записи |
| `window/source/compare.rs`, `files.rs` | сравнение для окна; файлы детали |
| `window/source/machine.rs`, `words.rs` | git и имя автора; ошибки и масса словами |
| `window/scenarios.rs` | сценарии человека: настоящее окно и настоящий проект на git |
| `window_lock.rs` | один проект в одном окне |
| `welcome.rs` | подсказка, если окно не открылось или отключено |
| `format.rs` | значки изменений, номер версии, дата |
| `commands/project.rs` | `init`, `part new`, `list`, `info`, `overview`, `agents`, `modules` |
| `commands/layout.rs` | `map`, `folder new`, `part move`, `upgrade` |
| `commands/import.rs` | `import` |
| `commands/intake.rs` | `intake`: план разбора папки и его выполнение (`--apply`) |
| `commands/history.rs` | `status`, `commit`, `log`, `restore`, `hide`, `unhide`, `export` |
| `commands/files.rs` | `files`, `show`, `file add`, `file remove`, `open` |
| `commands/compare.rs` | `diff`, `render` |
| `commands/passport.rs` | `material`, `materials` |
| `commands/json/mod.rs`, `view.rs` | ответ `--json`, код ошибки, выход; вид данных в JSON |
| `commands/mcp.rs` | `mcp`: сервер MCP, вызов инструмента — это команда с `--json` |
| `commands/describe.rs` | `describe`: команды и коды ошибок из разбора команд |

## Тесты

| Где | Что проверяют |
|---|---|
| `#[cfg(test)] mod tests` в файле | мелкие функции этого файла |
| `nonius-core/tests/project.rs` | папка проекта целиком |
| `nonius-store/tests/store.rs` | версии на настоящем git |
| `nonius-store/tests/layout.rs` | перенос детали, возврат в свою папку, обновление старого проекта с историей |
| `nonius-occt/tests/occt.rs`, `without_occt.rs` | геометрия с OpenCascade и сборка без него |
| `nonius-occt/tests/scene.rs` | сцена изменений на настоящем OpenCascade |
| `nonius-scene/tests/scene.rs`, `fixtures/scene-v*.glb` | формат сцены, контракт, чтение старых версий формата |
| `nonius-gui/tests/window.rs` | окно без экрана: детали → сравнение → места изменений, история и масса, сохранение первой версии, меню «Файл», смена проекта, новая деталь, окошко «Добавить модель», очередь файлов, битый файл, полоса ошибки, «Считаю…», возврат в окно, первый запуск, дерево, карта, выделение (Ctrl, Shift, рамка), удаление нескольких и «Остановить», клавиши выделенного, стрелки в окошке и меню |
| `nonius-gui/tests/render.rs` | 3D-отрисовка в память: цвета режимов |
| `nonius-occt/tests/shared_faces.rs` | сравнение почти одинаковых версий, местная разница против разницы целиком, кэш STEP |
| `nonius-occt/tests/honest.rs` | маленькое отверстие в большой детали не теряется, слой тоньше допуска — шум, файл без твёрдых тел |
| `nonius-app/tests/compare_rules.rs` | допуск проекта доходит до отрисовщиков; досчитанное остаётся в кэше при отмене |
| `nonius-occt/tests/features.rs` | изменения словами на настоящих телах |
| `nonius-app/tests/app.rs` | действия на временном проекте: статус, история, масса, сравнение и кэш, отмена, два потока |
| `nonius-app/src/intake/plan_tests.rs` | правила разбора на придуманных папках: версии по именам, нумерованные детали, чертёж по ссылке, папки сборок |
| `nonius-app/tests/intake.rs` | разбор настоящей папки на диске с поддельными связями: план, выполнение, версии в истории, повторный разбор |
| `nonius-app/tests/convert.rs` | импорт файла SolidWorks с поддельным модулем перевода: STEP рядом, отказ перевода, без модуля |
| `nonius-solidworks/tests/contract.rs` | контракт модуля; с настоящим SolidWorks — только если задать `NONIUS_TEST_SOLIDWORKS=путь к .sldprt` |
| `nonius-cli/tests/cli.rs` | команды `nonius` от начала до конца |
| `nonius-cli/tests/folders.rs` | `folder new`, `part move`, `map --json`, `upgrade` |

Карту обновляет тот, кто добавляет, переносит или удаляет файл, — в том же pull request.
