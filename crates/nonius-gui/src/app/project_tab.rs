//! Вкладка проекта: всё, что окно знает об одном открытом проекте. Окно
//! держит несколько вкладок, рисуется одна — открытая, остальные ждут.
//! У каждой своя очередь фоновых работ: результат не попадёт в чужую.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use super::jobs::Jobs;
use super::{
    add_model, batch, bottom, intake, material, new_folder, new_part, node, open_menu,
    part_dialogs, save, selection, settings_window, shown, structure,
};
use crate::source::{
    Comparison, DiffEntry, FileEntry, MassEntry, MaterialEntry, PartEntry, Pick, ProjectSource,
    VersionEntry,
};

/// Один проект в окне.
#[derive(Default)]
pub(super) struct ProjectTab {
    /// Открытый проект; `None` — начальный экран.
    pub project: Option<Arc<dyn ProjectSource>>,
    pub parts: Vec<PartEntry>,
    pub programs: open_menu::Programs,
    pub part: Option<String>,
    pub versions: Vec<VersionEntry>,
    pub picks: (Pick, Pick),
    pub want: Option<(String, Option<(Pick, Pick)>)>,
    pub comparison: Option<Comparison>,
    pub shown: Option<(String, (Pick, Pick))>,
    /// Деталь, под которую настроена камера: её вид не сбрасывается.
    pub framed: Option<String>,
    pub selected: Option<usize>,
    pub jobs: Jobs,
    /// Ошибка — показывается в строке состояния, пока не выбрали другое.
    pub message: Option<String>,
    /// Что получилось: «Сохранена версия …», «Сравнение отменено».
    pub notice: Option<String>,
    /// Что сказать, когда деталь будет показана.
    pub sticky: Option<String>,
    /// Строка поиска в дереве.
    pub search: String,
    /// Раскрытые в дереве детали.
    pub expanded: BTreeSet<String>,
    /// Папки-группы проекта: путь от корня через `/`.
    pub folders: Vec<String>,
    /// Обычные файлы в папках-группах: путь от корня через `/`.
    pub loose: Vec<String>,
    /// Выделенное в дереве и «Структуре»: его удаляет Delete.
    pub selection: selection::Selection,
    /// Последним выбрали файл на вкладке «Файлы»: Delete — про него, а не
    /// про выделенное в дереве.
    pub keys_to_files: bool,
    /// Порядок предметов на экране для Ctrl+A: дерево и «Структура».
    pub tree_order: Vec<node::Node>,
    pub structure_order: Vec<node::Node>,
    /// Последний щелчок был в «Структуре»: Ctrl+A выделяет там.
    pub in_structure: bool,
    /// Идёт очередь: перенос или удаление нескольких.
    pub batch: Option<batch::Batch>,
    /// Открытый экран «Разобрать папку» и счётчик разборов.
    pub intake: Option<intake::Intake>,
    pub intake_runs: u64,
    /// Что где на экране: строки дерева и «Структуры» с прошлого кадра.
    pub on_screen: Vec<shown::Shown>,
    /// Куда ляжет файл из Проводника: запомнено, пока его несли над окном.
    pub drop_aim: Option<node::Node>,
    /// Над какой свёрнутой папкой держат курсор и с какого мига.
    pub hover_since: Option<(String, f64)>,
    /// Свёрнутые в дереве папки; новые видны раскрытыми.
    pub folded: BTreeSet<String>,
    /// Куда класть новую деталь или папку: последняя выбранная папка,
    /// пустая строка — корень.
    pub place: String,
    /// Открытое окошко «Новая папка».
    pub new_folder: Option<new_folder::NewFolder>,
    /// Версии раскрытых деталей, кроме выбранной.
    pub listed: HashMap<String, Vec<VersionEntry>>,
    /// С чем сравнивается деталь вне истории: имя файла или детали.
    pub outside: Option<String>,
    /// Открытое окошко действия над деталью.
    pub dialog: Option<part_dialogs::Dialog>,
    pub materials: Vec<MaterialEntry>,
    /// Материал и масса для пары версий: «Было», «Стало», сейчас.
    pub masses: Option<((Pick, Pick), [MassEntry; 3])>,
    /// Открытое окошко «Сохранить версию».
    pub save: Option<save::SaveDialog>,
    /// Открытое окошко «Новая деталь».
    pub new_part: Option<new_part::NewPart>,
    /// Открытое окошко «Добавить модель».
    pub add: Option<add_model::AddModel>,
    /// Что добавить, когда откроется новый проект.
    pub after_open: Vec<PathBuf>,
    /// Вернулись в окно: перечитать детали, как только оно освободится.
    pub reread_wanted: bool,
    /// Ввод своего материала.
    pub custom: material::Custom,
    /// Материал записывается — список выбора ждёт.
    pub material_busy: bool,
    /// Открытая вкладка под 3D; `None` — панель свёрнута.
    pub panel: Option<bottom::Tab>,
    /// Файлы для пары версий.
    pub files: Option<((Pick, Pick), Vec<FileEntry>)>,
    /// Выбранный файл на вкладке «Файлы».
    pub file: Option<String>,
    /// Строки выбранного текстового файла.
    pub file_text: Option<Vec<DiffEntry>>,
    /// Черновик сохраняется в эту папку, как только вкладка освободится.
    pub save_to: Option<PathBuf>,
    /// Проект старого формата только что открыт: спросить, обновить ли.
    pub ask_upgrade: bool,
    /// Проект обновляется, как только вкладка освободится.
    pub upgrading: bool,
    /// Настройки проекта записаны: открыть его заново, как только вкладка
    /// освободится.
    pub reopen: bool,
    /// Что показано в центре: деталь в 3D или структура проекта.
    pub view: structure::View,
    /// Открытое окно «Настройки».
    pub settings: Option<settings_window::SettingsDialog>,
}

impl ProjectTab {
    /// Фоновая работа упала, и неизвестно какая: всё, что ждёт результата,
    /// снова свободно и говорит почему. Иначе кнопки остались бы серыми
    /// навсегда.
    pub fn stop_waiting(&mut self, why: &str) {
        self.material_busy = false;
        self.intake_crashed();
        if let Some(d) = self.save.as_mut().filter(|d| d.saving) {
            d.saving = false;
            d.error = Some(why.to_string());
        }
        if let Some(d) = self.dialog.as_mut().filter(|d| d.busy()) {
            d.failed(why.to_string());
        }
        if let Some(d) = self.add.as_mut().filter(|d| d.busy()) {
            d.failed(why.to_string());
        }
        if let Some(d) = self.new_part.as_mut().filter(|d| d.busy()) {
            d.failed(why.to_string());
        }
        if let Some(d) = self.new_folder.as_mut().filter(|d| d.busy()) {
            d.failed(why.to_string());
        }
        if let Some(d) = self.settings.as_mut().filter(|d| d.busy()) {
            d.failed(why.to_string());
        }
    }

    /// Проект во вкладке больше не нужен (вкладку закрыли или открыли в ней
    /// другой): очередь останавливается после текущего предмета, результаты
    /// работ забываются.
    pub fn abandon(&mut self) {
        if let Some(batch) = self.batch.take() {
            batch.stop();
        }
        if let Some(intake) = self.intake.take() {
            intake.stop();
        }
        self.jobs.forget_all();
    }
}
