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
    /// Подключённые модули.
    Modules,
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
}
