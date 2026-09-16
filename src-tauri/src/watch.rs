use notify::{
    event::{EventKind, ModifyKind},
    Event, RecommendedWatcher, RecursiveMode, Watcher,
};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Component, Path};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

#[derive(Default)]
pub struct WatchHub {
    watchers: Mutex<HashMap<String, RecommendedWatcher>>,
}

fn watch_key(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

fn skip_worktree(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | "target"
            | "dist"
            | ".idea"
            | ".vscode"
            | "__pycache__"
            | ".next"
            | "vendor"
            | ".cache"
    )
}

fn skip_git(name: &str) -> bool {
    matches!(name, "objects" | "logs" | "hooks" | "info" | "modules" | "rr-cache")
}

fn classify(path: &Path) -> Option<&'static str> {
    let names: Vec<&str> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    if names.iter().any(|name| name.ends_with(".lock")) {
        return None;
    }
    if let Some(index) = names.iter().position(|name| name.eq_ignore_ascii_case(".git")) {
        let rest = names.get(index + 1).copied().unwrap_or("");
        if rest.is_empty() {
            return Some("git");
        }
        if skip_git(rest) {
            return None;
        }
        return Some("git");
    }
    if names.iter().copied().any(skip_worktree) {
        return None;
    }
    Some("worktree")
}

fn interesting(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Name(_) | ModifyKind::Any)
            | EventKind::Any
    )
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkdirChanged {
    path: String,
    kind: &'static str,
}

pub fn watch_repo(hub: &WatchHub, app: AppHandle, path: String) -> Result<(), String> {
    let key = watch_key(&path);
    let mut map = hub.watchers.lock().map_err(|_| "状态锁失败".to_string())?;
    if map.contains_key(&key) {
        return Ok(());
    }
    let emit_path = path.clone();
    let mut watcher = notify::recommended_watcher(move |result: Result<Event, notify::Error>| {
        let Ok(event) = result else {
            return;
        };
        if !interesting(&event.kind) {
            return;
        }
        let mut kind = None;
        for item in &event.paths {
            match classify(item) {
                Some("git") => {
                    kind = Some("git");
                    break;
                }
                Some(value) => kind = Some(value),
                None => {}
            }
        }
        let Some(kind) = kind else {
            return;
        };
        let _ = app.emit(
            "workdir-changed",
            WorkdirChanged {
                path: emit_path.clone(),
                kind,
            },
        );
    })
    .map_err(|error| format!("无法监听工作区：{error}"))?;
    watcher
        .watch(Path::new(&path), RecursiveMode::Recursive)
        .map_err(|error| format!("无法监听工作区：{error}"))?;
    map.insert(key, watcher);
    Ok(())
}

pub fn unwatch_repo(hub: &WatchHub, path: String) -> Result<(), String> {
    let key = watch_key(&path);
    hub.watchers
        .lock()
        .map_err(|_| "状态锁失败".to_string())?
        .remove(&key);
    Ok(())
}
