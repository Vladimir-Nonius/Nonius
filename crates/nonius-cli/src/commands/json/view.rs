//! Как данные программы выглядят в JSON: версия, деталь, масса, слова состояний.

use nonius_app::{FileChangeKind, ItemState, ItemStatus, Mass, VersionInfo};
use nonius_core::meta::{ItemKind, PartMeta};
use serde_json::{json, Value};

pub fn item(s: &ItemStatus) -> Value {
    let changes: Vec<Value> = s
        .changes
        .iter()
        .map(|c| json!({ "path": c.path, "change": change(c.kind) }))
        .collect();
    json!({
        "name": s.item.name.as_str(),
        "kind": kind(s.item.kind),
        "state": match s.state() {
            ItemState::New => "new",
            ItemState::Changed => "changed",
            ItemState::Saved => "saved",
        },
        "versions": s.versions,
        "latest": s.latest.as_ref().map(version),
        "changes": changes,
    })
}

pub fn version(v: &VersionInfo) -> Value {
    let ai =
        v.ai.as_ref()
            .map(|ai| json!({ "name": ai.name(), "model": ai.model() }));
    json!({
        "ref": format!("{}@{}", v.item, v.version),
        "item": v.item.as_str(),
        "version": v.version.to_string(),
        "commit": v.commit,
        "author": v.author,
        "committer": v.committer,
        "ai": ai,
        "date": v.date,
        "summary": v.summary(),
        "message": v.message,
    })
}

pub fn mass(meta: &PartMeta) -> Value {
    match Mass::of(meta) {
        Mass::Computed { kg, density_kg_m3 } => {
            json!({ "kg": kg, "source": "computed", "density_kg_m3": density_kg_m3 })
        }
        Mass::FromCad { kg } => json!({ "kg": kg, "source": "cad" }),
        Mass::NoMaterial => json!({ "kg": null, "missing": "material" }),
        Mass::NoDensity { .. } => json!({ "kg": null, "missing": "density" }),
        Mass::NoVolume => json!({ "kg": null, "missing": "volume" }),
    }
}

pub fn kind(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Part => "part",
        ItemKind::Assembly => "assembly",
    }
}

pub fn change(kind: FileChangeKind) -> &'static str {
    match kind {
        FileChangeKind::Added => "added",
        FileChangeKind::Modified => "modified",
        FileChangeKind::Deleted => "deleted",
    }
}
