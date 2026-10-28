//! Подсказка, если окно программы не открылось или отключено
//! (`NONIUS_NO_WINDOW=1`): как работать из командной строки.

use std::io::IsTerminal;

use clap::CommandFactory;

use crate::args::Cli;

/// Подсказка, если окно не открылось. При двойном щелчке Windows закрывает
/// окно консоли сразу после выхода, поэтому там ждём Enter.
pub fn welcome() {
    println!(
        "Nonius {} — контроль версий для CAD-моделей.\n\n\
         Окно программы открывается двойным щелчком по nonius.exe или командой \
         nonius view. Всё то же доступно из командной строки.\n\
         Как открыть PowerShell в папке с nonius.exe:\n  \
         1. Откройте эту папку в Проводнике.\n  \
         2. Щёлкните в адресной строке, наберите powershell и нажмите Enter.\n  \
         3. Введите: .\\nonius.exe --help\n",
        env!("CARGO_PKG_VERSION")
    );
    println!("{}", Cli::command().render_help());
    if cfg!(windows) && std::io::stdin().is_terminal() {
        println!("Нажмите Enter, чтобы закрыть окно.");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
}
