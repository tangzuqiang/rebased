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

fn normalize_full(path: &str) -> String {
    let stripped = path
        .strip_prefix(r"\\?\")
        .or_else(|| path.strip_prefix("//?/"))
        .unwrap_or(path);
    stripped.replace('\\', "/")
}

fn rel_under(root: &str, path: &Path) -> Option<String> {
    let root = normalize_full(root).trim_end_matches('/').to_string();
    let full = normalize_full(&path.to_string_lossy()).trim_end_matches('/').to_string();
    if root.is_empty() || full.len() < root.len() {
        return None;
    }
    if !full[..root.len()].eq_ignore_ascii_case(&root) {
        return None;
    }
    if full.len() == root.len() {
        return None;
    }
    if full.as_bytes().get(root.len()) != Some(&b'/') {
        return None;
    }
    let rel = full[root.len() + 1..].trim_matches('/');
    if rel.is_empty() {
        None
    } else {
        Some(rel.to_string())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkdirChanged {
    path: String,
    kind: &'static str,
    paths: Vec<String>,
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
        let mut saw_git = false;
        let mut saw_worktree = false;
        let mut overflow = false;
        let mut paths = Vec::new();
        for item in &event.paths {
            match classify(item) {
                Some("git") => saw_git = true,
                Some("worktree") => {
                    saw_worktree = true;
                    if overflow {
                        continue;
                    }
                    if paths.len() >= 64 {
                        overflow = true;
                        paths.clear();
                        continue;
                    }
                    if let Some(rel) = rel_under(&emit_path, item) {
                        if !paths.iter().any(|existing| existing == &rel) {
                            paths.push(rel);
                        }
                    }
                }
                _ => {}
            }
        }
        let kind = if saw_git {
            "git"
        } else if saw_worktree {
            "worktree"
        } else {
            return;
        };
        let _ = app.emit(
            "workdir-changed",
            WorkdirChanged {
                path: emit_path.clone(),
                kind,
                paths,
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

#[cfg(test)]
mod tests {
    use super::rel_under;
    use std::path::Path;

    #[test]
    fn relative_path_strips_repo_root() {
        let rel = rel_under(r"D:\Repo", Path::new(r"\\?\D:\Repo\src\App.vue")).unwrap();
        assert_eq!(rel, "src/App.vue");
    }

    #[test]
    fn relative_path_ignores_ascii_case() {
        let rel = rel_under("D:/repo", Path::new("d:/Repo/README.md")).unwrap();
        assert_eq!(rel, "README.md");
    }

    #[test]
    fn relative_path_rejects_sibling_prefix() {
        assert!(rel_under(r"D:\Repo", Path::new(r"D:\Repo2\file.txt")).is_none());
    }
}
