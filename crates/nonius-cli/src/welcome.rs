//! Подсказка при запуске без команды (двойной щелчок по nonius.exe).

use std::io::IsTerminal;

use clap::CommandFactory;

use crate::args::Cli;

/// Подсказка при запуске без команды. При двойном щелчке Windows закрывает
/// окно сразу после выхода, поэтому там ждём Enter.
pub fn welcome() {
    println!(
        "Nonius {} — контроль версий для CAD-моделей.\n\n\
         Это программа для командной строки: её запускают из PowerShell, \
         а не двойным щелчком.\n\
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
