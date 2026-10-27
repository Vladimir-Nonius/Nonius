# CLAUDE.md

Правила работы в репозитории — в [AGENTS.md](AGENTS.md), архитектура — в [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Прочитай оба файла перед изменениями. Какие файлы открывать под задачу — в [docs/CODEMAP.md](docs/CODEMAP.md); весь код не читай.

Коротко:
- Rust workspace: `crates/nonius-core` (ядро), `crates/nonius-store` (версии на git + LFS), `crates/nonius-step` (заголовок STEP), `crates/nonius-occt` (геометрия OpenCascade), `crates/nonius-cli` (команда `nonius`).
- Тестам хранилища нужны `git` и `git-lfs` в PATH.
- Проверки: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
- Новые модули — отдельными крейтами через трейты `nonius_core::extension` + контрактный тест из `nonius_core::contract`.
- Схемы файлов (`project.yaml`, `meta.yaml`, `tree.yaml`) меняются только с миграцией и тестом.
- Файлы небольшие: одна тема, до ~300 строк без тестов (правило 9 в AGENTS.md).
- Тексты для пользователя — по-русски.
- Работай в ветке `claude/<задача>`, в `main` не пушь; результат — pull request.
