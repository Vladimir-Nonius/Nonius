//! Команды и их параметры (clap). Текст `///` — это справка `nonius --help`.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Nonius — контроль версий для CAD-моделей.
#[derive(Parser)]
#[command(name = "nonius", version, about, long_about = None)]
pub struct Cli {
    /// Папка проекта (по умолчанию — текущая или ближайшая выше).
    #[arg(short = 'C', long, global = true)]
    pub project: Option<PathBuf>,

    /// Действует ИИ: его имя пишется автором версии. Запустивший человек тоже остаётся в истории.
    #[arg(long, global = true, env = "NONIUS_AI", value_name = "ИМЯ")]
    pub ai: Option<String>,

    /// Модель ИИ: пишется в версию рядом с именем.
    #[arg(long, global = true, env = "NONIUS_AI_MODEL", value_name = "МОДЕЛЬ")]
    pub ai_model: Option<String>,

    /// Ответить в JSON для программ и ИИ (какие команды умеют: nonius describe).
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Создать проект в папке.
    Init {
        /// Папка (по умолчанию текущая).
        path: Option<PathBuf>,
        /// Название проекта (по умолчанию имя папки).
        #[arg(long)]
        name: Option<String>,
    },
    /// Детали и сборки.
    #[command(subcommand)]
    Part(PartCommand),
    /// Список деталей и сборок.
    List,
    /// Сведения о проекте.
    Info,
    /// Весь проект одним ответом: детали, последние версии, что не сохранено.
    Overview {
        /// Только детали с несохранёнными изменениями.
        #[arg(long)]
        changed: bool,
    },
    /// Подключённые модули.
    Modules,
    /// Сервер MCP для ИИ: те же действия, что у команд. Без --write только чтение.
    Mcp {
        /// Разрешить менять проект: фиксировать версии, откатывать, импортировать.
        #[arg(long)]
        write: bool,
    },
    /// Обновить правила для ИИ в AGENTS.md: свой текст остаётся.
    Agents,
    /// Все команды, параметры и коды ошибок в JSON: для программ и ИИ.
    Describe,
    /// Добавить файл модели (STEP, SLDPRT, SLDASM) в деталь.
    Import {
        /// Файл модели.
        file: PathBuf,
        /// Деталь (по умолчанию — имя файла без расширения; создаётся, если нет).
        #[arg(long)]
        part: Option<String>,
    },
    /// Что изменилось с последней версии.
    Status {
        /// Деталь (по умолчанию — все).
        part: Option<String>,
    },
    /// Зафиксировать новую версию детали.
    Commit {
        /// Деталь.
        part: String,
        /// Описание: что и почему изменено.
        #[arg(short, long)]
        message: String,
    },
    /// История версий детали.
    Log {
        /// Деталь.
        part: String,
    },
    /// Сравнить версии: по умолчанию последнюю с текущим состоянием.
    Diff {
        /// Деталь.
        part: String,
        /// С какой версии (по умолчанию последняя).
        from: Option<String>,
        /// По какую версию (по умолчанию текущее состояние).
        to: Option<String>,
    },
    /// Сравнить деталь с файлом STEP или с другой деталью, ничего не сохраняя.
    Compare {
        /// Деталь.
        part: String,
        /// Какую версию детали взять (по умолчанию текущее состояние).
        #[arg(long)]
        version: Option<String>,
        /// Файл STEP на диске.
        #[arg(long, required_unless_present = "with", conflicts_with = "with")]
        file: Option<PathBuf>,
        /// Другая деталь, как она есть сейчас.
        #[arg(long)]
        with: Option<String>,
    },
    /// Вернуть деталь к старой версии (создаётся новая версия, история сохраняется).
    Restore {
        /// Деталь.
        part: String,
        /// Версия: v3 или 3.
        version: String,
        /// Почему откатываем.
        #[arg(short, long)]
        message: Option<String>,
    },
    /// Нарисовать изменения цветом: зелёное — добавлено, красное — убрано.
    Render {
        /// Деталь.
        part: String,
        /// С какой версии (по умолчанию последняя).
        from: Option<String>,
        /// По какую версию (по умолчанию текущее состояние).
        to: Option<String>,
        /// Куда сохранить картинку (по умолчанию <деталь>-<было>-<стало>.png).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Открыть окно программы: детали, версии, изменения в 3D.
    View {
        /// Сразу показать эту деталь.
        part: Option<String>,
        /// С какой версии (по умолчанию — как выберет окно).
        from: Option<String>,
        /// По какую версию (по умолчанию текущее состояние).
        to: Option<String>,
    },
    /// Выгрузить файлы версии в папку.
    Export {
        /// Деталь.
        part: String,
        /// Версия: v3 или 3.
        version: String,
        /// Куда выгрузить.
        dest: PathBuf,
    },
    /// Файлы детали: что изменилось между версиями, какого размера.
    Files {
        /// Деталь.
        part: String,
        /// С какой версии (по умолчанию последняя).
        from: Option<String>,
        /// По какую версию (по умолчанию текущее состояние).
        to: Option<String>,
        /// Показать, какие строки изменились в этом текстовом файле.
        #[arg(long)]
        text: Option<String>,
    },
    /// Показать текстовый файл детали целиком: как сейчас или в версии.
    Show {
        /// Деталь.
        part: String,
        /// Файл внутри детали: docs/технология.md.
        file: String,
        /// Версия: v3 или 3 (по умолчанию — как сейчас в папке).
        #[arg(long)]
        version: Option<String>,
    },
    /// Свои файлы детали: технология, чертёж, ТЗ.
    #[command(subcommand)]
    File(FileCommand),
    /// Открыть файл детали: в программе, назначенной в системе, в другой, или показать в папке.
    Open {
        /// Деталь.
        part: String,
        /// Файл внутри детали: docs/чертёж.pdf.
        file: String,
        /// Версия: откроется копия, правки в ней не попадут в историю.
        #[arg(long)]
        version: Option<String>,
        /// Показать программы, которые система знает для этого файла.
        #[arg(long, conflicts_with_all = ["with", "folder"])]
        programs: bool,
        /// Открыть этой программой из списка `--programs`.
        #[arg(long, conflicts_with = "folder")]
        with: Option<String>,
        /// Не открывать, а показать файл в папке.
        #[arg(long)]
        folder: bool,
    },
    /// Материал и масса детали: показать, назначить (масса = объём × плотность) или убрать.
    Material {
        /// Деталь.
        part: String,
        /// Материал: Ст3, 09Г2С, АМг6… (без него — показать паспорт).
        material: Option<String>,
        /// Плотность, кг/м³ (по умолчанию — из справочника: nonius materials).
        #[arg(long, requires = "material")]
        density: Option<f64>,
        /// Убрать материал.
        #[arg(long, conflicts_with_all = ["material", "density"])]
        remove: bool,
    },
    /// Справочник материалов с плотностью.
    Materials,
}

#[derive(Subcommand)]
pub enum PartCommand {
    /// Добавить деталь (или сборку с --assembly).
    New {
        /// Имя детали: кириллица можно, / \ : * ? " < > | @ # [ ] ^ нельзя.
        name: String,
        /// Это сборка.
        #[arg(long)]
        assembly: bool,
    },
    /// Переименовать деталь: версии остаются при ней.
    Rename {
        /// Нынешнее имя.
        name: String,
        /// Новое имя.
        new: String,
    },
    /// Удалить деталь: она остаётся в истории, вернёт её `part recover`.
    Delete {
        /// Деталь.
        name: String,
    },
    /// Удалённые детали, которые можно вернуть.
    Deleted,
    /// Вернуть удалённую деталь.
    Recover {
        /// Деталь.
        name: String,
    },
}

#[derive(Subcommand)]
pub enum FileCommand {
    /// Положить копию файла в docs/ детали; в историю он попадёт со следующей версией.
    Add {
        /// Деталь.
        part: String,
        /// Файл на диске.
        file: PathBuf,
    },
    /// Убрать файл из папки детали: в сохранённых версиях он остаётся.
    Remove {
        /// Деталь.
        part: String,
        /// Файл внутри детали: docs/технология.md.
        file: String,
    },
}
