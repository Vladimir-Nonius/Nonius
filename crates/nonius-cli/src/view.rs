//! `nonius view` — окно просмотра: деталь крутится мышью, изменения цветом.

use anyhow::{bail, Context, Result};
use nonius_core::compare::ChangeReport;
use nonius_store::Store;
use nonius_view::{open_browser, Page, Viewer};

use super::{registry, snapshots};

/// Показать деталь в браузере и держать просмотр, пока программу не закроют.
pub(crate) fn view(
    store: &Store,
    part: &str,
    from: Option<&str>,
    to: Option<&str>,
    browser: bool,
) -> Result<()> {
    let item = store.item(part)?;
    let reg = registry();
    let Some(renderer) = reg
        .renderers()
        .find(|r| r.manifest().id == nonius_occt::SCENE_RENDERER)
    else {
        bail!("объёмный просмотр доступен в сборке Nonius с OpenCascade");
    };

    // Версий ещё нет — показываем то, что лежит в папке, без сравнения.
    let (before, after, subtitle, compare) =
        if from.is_none() && to.is_none() && store.latest(&item)?.is_none() {
            let now = store.working_snapshot(&item)?;
            (
                now.clone(),
                now,
                "текущее состояние, версий ещё нет".to_string(),
                false,
            )
        } else {
            let (before, after, from_label, to_label) = snapshots(store, &item, from, to)?;
            (before, after, format!("{from_label} → {to_label}"), true)
        };

    println!("«{}»: {subtitle}", item.name);
    println!("Готовлю объёмную модель…");
    let work = store
        .project()
        .root()
        .join(".git")
        .join("nonius")
        .join("view");
    let files = renderer.render(&before, &after, &ChangeReport::empty(""), &work)?;
    let Some(model) = files.first() else {
        bail!("у «{}» нет model.step — показывать нечего", item.name);
    };
    let model = std::fs::read(model)
        .with_context(|| format!("не удалось прочитать {}", model.display()))?;

    let viewer = Viewer::start(Page {
        title: item.name.to_string(),
        subtitle,
        model,
    })
    .context("не удалось запустить просмотр")?;
    let url = viewer.url();
    println!("Просмотр: {url}");
    if browser {
        match open_browser(&url) {
            Ok(()) => println!("Открыл в браузере."),
            Err(_) => println!("Браузер не открылся — скопируйте адрес выше в браузер."),
        }
    }
    if compare {
        println!("Зелёное — добавлено, красное — убрано, серое — без изменений.");
    }
    println!(
        "Пока это окно открыто, просмотр работает. Закончить — закройте окно или нажмите Ctrl+C."
    );
    viewer.run();
    Ok(())
}
