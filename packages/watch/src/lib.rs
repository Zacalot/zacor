use chrono::Utc;
use notify::{EventKind, RecursiveMode, Watcher};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::mpsc;

zacor_package::include_args!();

#[derive(Serialize)]
pub struct WatchRecord {
    pub event: String,
    pub path: String,
    pub time: String,
}

fn event_kind_name(kind: &EventKind) -> Option<&'static str> {
    match kind {
        EventKind::Create(_) => Some("create"),
        EventKind::Modify(_) => Some("modify"),
        EventKind::Remove(_) => Some("remove"),
        _ => None,
    }
}

pub fn watch(
    path: PathBuf,
    no_recursive: bool,
) -> Result<impl Iterator<Item = WatchRecord>, String> {
    let (tx, rx) = mpsc::channel();

    let mode = if no_recursive {
        RecursiveMode::NonRecursive
    } else {
        RecursiveMode::Recursive
    };

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            if let Some(name) = event_kind_name(&event.kind) {
                for path in &event.paths {
                    let _ = tx.send(WatchRecord {
                        event: name.to_string(),
                        path: path.display().to_string(),
                        time: Utc::now().to_rfc3339(),
                    });
                }
            }
        }
    })
    .map_err(|e| format!("watch: failed to create watcher: {e}"))?;

    let watch_path = path.canonicalize().unwrap_or(path);
    watcher
        .watch(&watch_path, mode)
        .map_err(|e| format!("watch: failed to watch {}: {e}", watch_path.display()))?;

    // Leak the watcher so it stays alive for the lifetime of the iterator
    std::mem::forget(watcher);

    Ok(rx.into_iter())
}
