mod ai;
mod git;
mod store;
mod term;
mod updater;
mod watch;

use git::{
    AppState, BlameLine, BranchInfo, FileDiff, FilePreview, GitCommit, GitFile, MergeCompare, MergeFileContent,
    MergeOutcome, MergePreview, MergeState, RemoteDetail, RepoInfo, StatusPayload,
};
use std::path::PathBuf;
use store::{AppStore, RecentProject, SessionState};
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Manager, State};

fn repo_dir(state: &AppState, repo: Option<String>) -> Result<PathBuf, String> {
    state.resolve_repo(repo.as_deref())
}

async fn run_blocking<T, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    spawn_blocking(task)
        .await
        .map_err(|error| format!("后台任务失败：{error}"))?
}

#[tauri::command]
async fn open_repo(state: State<'_, AppState>, path: String) -> Result<RepoInfo, String> {
    let (repo, info) = run_blocking(move || git::discover_repo(&path)).await?;
    state.set_repo(repo)?;
    Ok(info)
}

#[tauri::command]
async fn get_repo(state: State<'_, AppState>, repo: Option<String>) -> Result<RepoInfo, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::repo_info(&path)).await
}

#[tauri::command]
async fn get_status(state: State<'_, AppState>, repo: Option<String>) -> Result<StatusPayload, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::status(&path)).await
}

#[tauri::command]
async fn get_log(
    state: State<'_, AppState>,
    limit: Option<u32>,
    repo: Option<String>,
    refs: Option<Vec<String>>,
) -> Result<Vec<GitCommit>, String> {
    let path = repo_dir(&state, repo)?;
    let refs = refs.unwrap_or_default();
    let limit = limit.unwrap_or(200);
    run_blocking(move || git::log(&path, limit, &refs)).await
}

#[tauri::command]
async fn get_commit_files(
    state: State<'_, AppState>,
    hash: String,
    repo: Option<String>,
) -> Result<Vec<GitFile>, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::commit_files(&path, &hash)).await
}

#[tauri::command]
async fn get_branches(state: State<'_, AppState>, repo: Option<String>) -> Result<Vec<BranchInfo>, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::branches(&path)).await
}

#[tauri::command]
async fn get_recent_branches(
    state: State<'_, AppState>,
    limit: Option<u32>,
    repo: Option<String>,
) -> Result<Vec<String>, String> {
    let path = repo_dir(&state, repo)?;
    let limit = limit.unwrap_or(4) as usize;
    run_blocking(move || git::recent_branches(&path, limit)).await
}

#[tauri::command]
async fn stage_file(state: State<'_, AppState>, path: String, repo: Option<String>) -> Result<(), String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::stage(&repo, &path)).await
}

#[tauri::command]
async fn unstage_file(state: State<'_, AppState>, path: String, repo: Option<String>) -> Result<(), String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::unstage(&repo, &path)).await
}

#[tauri::command]
async fn commit_changes(
    state: State<'_, AppState>,
    message: String,
    amend: bool,
    paths: Vec<String>,
    repo: Option<String>,
) -> Result<(), String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::commit(&repo, &message, amend, &paths)).await
}

#[tauri::command]
async fn checkout_branch(state: State<'_, AppState>, name: String, repo: Option<String>) -> Result<(), String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::checkout(&path, &name)).await
}

#[tauri::command]
async fn fetch_remote(state: State<'_, AppState>, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::fetch(&path)).await
}

#[tauri::command]
async fn pull_remote(state: State<'_, AppState>, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::pull(&path)).await
}

#[tauri::command]
async fn push_remote(state: State<'_, AppState>, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::push(&path)).await
}

#[tauri::command]
async fn get_remotes(state: State<'_, AppState>, repo: Option<String>) -> Result<Vec<String>, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::remotes(&path)).await
}

#[tauri::command]
async fn push_to(
    state: State<'_, AppState>,
    remote: String,
    branch: String,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::push_to(&path, &remote, &branch)).await
}

#[tauri::command]
async fn get_merge_preview(
    state: State<'_, AppState>,
    theirs: String,
    repo: Option<String>,
) -> Result<MergePreview, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::merge_preview(&path, &theirs)).await
}

#[tauri::command]
async fn get_merge_file(
    state: State<'_, AppState>,
    theirs: String,
    path: String,
    repo: Option<String>,
) -> Result<MergeFileContent, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::merge_file(&repo, &theirs, &path)).await
}

#[tauri::command]
async fn compare_merge(
    state: State<'_, AppState>,
    theirs: String,
    repo: Option<String>,
) -> Result<MergeCompare, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::compare_merge(&path, &theirs)).await
}

#[tauri::command]
async fn merge_branch(
    state: State<'_, AppState>,
    theirs: String,
    repo: Option<String>,
) -> Result<MergeOutcome, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::merge_branch(&path, &theirs)).await
}

#[tauri::command]
async fn merge_abort(state: State<'_, AppState>, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::merge_abort(&path)).await
}

#[tauri::command]
async fn get_merge_state(state: State<'_, AppState>, repo: Option<String>) -> Result<MergeState, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::merge_state(&path)).await
}

#[tauri::command]
async fn merge_continue(state: State<'_, AppState>, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::merge_continue(&path)).await
}

#[tauri::command]
async fn resolve_conflict(
    state: State<'_, AppState>,
    path: String,
    side: String,
    repo: Option<String>,
) -> Result<MergeState, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::resolve_conflict(&repo, &path, &side)).await
}

#[tauri::command]
async fn write_resolved(
    state: State<'_, AppState>,
    path: String,
    content: String,
    repo: Option<String>,
) -> Result<MergeState, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::write_resolved(&repo, &path, &content)).await
}

#[tauri::command]
async fn get_conflict_file(
    state: State<'_, AppState>,
    path: String,
    repo: Option<String>,
) -> Result<MergeFileContent, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::conflict_file(&repo, &path)).await
}

#[tauri::command]
async fn get_working_diff(
    state: State<'_, AppState>,
    path: String,
    untracked: bool,
    repo: Option<String>,
) -> Result<FileDiff, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::working_diff(&repo, &path, untracked)).await
}

#[tauri::command]
async fn generate_commit_message(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<ai::DiffPath>,
    repo: Option<String>,
) -> Result<ai::GeneratedMessage, String> {
    let repo = Some(repo_dir(&state, repo)?.to_string_lossy().into_owned());
    run_blocking(move || {
        let state = app.state::<AppState>();
        let store = app.state::<AppStore>();
        ai::generate_commit_message(&state, &store, &paths, repo.as_deref())
    })
    .await
}

#[tauri::command]
async fn get_ai_config(app: AppHandle) -> Result<ai::AiConfig, String> {
    run_blocking(move || Ok(ai::get_ai_config(&app.state::<AppStore>()))).await
}

#[tauri::command]
async fn save_ai_config(app: AppHandle, config: ai::AiConfig) -> Result<ai::AiConfig, String> {
    run_blocking(move || ai::save_ai_config(&app.state::<AppStore>(), config)).await
}

#[tauri::command]
async fn test_ai_config(app: AppHandle) -> Result<String, String> {
    run_blocking(move || ai::test_ai_config(&app.state::<AppStore>())).await
}

#[tauri::command]
async fn get_recent_projects(app: AppHandle) -> Result<Vec<RecentProject>, String> {
    run_blocking(move || app.state::<AppStore>().recent_projects()).await
}

#[tauri::command]
async fn record_recent_project(app: AppHandle, path: String, name: String) -> Result<Vec<RecentProject>, String> {
    run_blocking(move || app.state::<AppStore>().touch_recent(&path, &name)).await
}

#[tauri::command]
async fn remove_recent_project(app: AppHandle, path: String) -> Result<Vec<RecentProject>, String> {
    run_blocking(move || app.state::<AppStore>().remove_recent(&path)).await
}

#[tauri::command]
async fn get_session(app: AppHandle) -> Result<SessionState, String> {
    run_blocking(move || app.state::<AppStore>().get_session()).await
}

#[tauri::command]
async fn save_session(app: AppHandle, session: SessionState) -> Result<SessionState, String> {
    run_blocking(move || app.state::<AppStore>().save_session(session)).await
}

#[tauri::command]
async fn get_app_setting(app: AppHandle, key: String) -> Result<Option<String>, String> {
    run_blocking(move || app.state::<AppStore>().get_setting(&key)).await
}

#[tauri::command]
async fn set_app_setting(app: AppHandle, key: String, value: String) -> Result<(), String> {
    run_blocking(move || app.state::<AppStore>().set_setting(&key, &value)).await
}

#[tauri::command]
async fn cherry_pick(state: State<'_, AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::cherry_pick(&path, &hash)).await
}

#[tauri::command]
async fn checkout_revision(
    state: State<'_, AppState>,
    hash: String,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::checkout_revision(&path, &hash)).await
}

#[tauri::command]
async fn reset_to(
    state: State<'_, AppState>,
    hash: String,
    mode: String,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::reset_to(&path, &hash, &mode)).await
}

#[tauri::command]
async fn revert_commit(state: State<'_, AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::revert_commit(&path, &hash)).await
}

#[tauri::command]
async fn undo_commit(state: State<'_, AppState>, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::undo_commit(&path)).await
}

#[tauri::command]
async fn create_branch_at(
    state: State<'_, AppState>,
    name: String,
    hash: String,
    checkout: bool,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::create_branch_at(&path, &name, &hash, checkout)).await
}

#[tauri::command]
async fn create_tag_at(
    state: State<'_, AppState>,
    name: String,
    hash: String,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::create_tag_at(&path, &name, &hash)).await
}

#[tauri::command]
async fn format_patch(state: State<'_, AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::format_patch(&path, &hash)).await
}

#[tauri::command]
async fn write_text_file(path: String, content: String) -> Result<(), String> {
    run_blocking(move || git::write_text_file(&path, &content)).await
}

#[tauri::command]
async fn list_worktree(state: State<'_, AppState>, repo: Option<String>) -> Result<Vec<String>, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::list_worktree(&path)).await
}

#[tauri::command]
async fn abs_worktree_path(
    state: State<'_, AppState>,
    path: String,
    repo: Option<String>,
) -> Result<String, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::abs_worktree_path(&repo, &path)).await
}

#[tauri::command]
async fn reveal_in_os(state: State<'_, AppState>, path: String, repo: Option<String>) -> Result<(), String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::reveal_in_os(&repo, &path)).await
}

#[tauri::command]
async fn open_in_os(state: State<'_, AppState>, path: String, repo: Option<String>) -> Result<(), String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::open_in_os(&repo, &path)).await
}

#[tauri::command]
async fn get_file_blame(
    state: State<'_, AppState>,
    path: String,
    repo: Option<String>,
) -> Result<Vec<BlameLine>, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::worktree_blame(&repo, &path)).await
}

#[tauri::command]
async fn preview_file(
    state: State<'_, AppState>,
    path: String,
    repo: Option<String>,
) -> Result<FilePreview, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::preview_file(&repo, &path)).await
}

#[tauri::command]
async fn save_worktree_file(
    state: State<'_, AppState>,
    path: String,
    content: String,
    repo: Option<String>,
) -> Result<FilePreview, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::save_worktree_file(&repo, &path, &content)).await
}

#[tauri::command]
async fn copy_worktree(
    state: State<'_, AppState>,
    src: String,
    dest: String,
    repo: Option<String>,
) -> Result<String, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::copy_worktree(&repo, &src, &dest)).await
}

#[tauri::command]
async fn copy_worktree_to_clipboard(
    state: State<'_, AppState>,
    path: String,
    repo: Option<String>,
) -> Result<(), String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::copy_worktree_to_clipboard(&repo, &path)).await
}

#[tauri::command]
async fn rename_worktree(
    state: State<'_, AppState>,
    src: String,
    dest: String,
    repo: Option<String>,
) -> Result<String, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::rename_worktree(&repo, &src, &dest)).await
}

#[tauri::command]
async fn delete_worktree(state: State<'_, AppState>, path: String, repo: Option<String>) -> Result<(), String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::delete_worktree(&repo, &path)).await
}

#[tauri::command]
async fn drop_commit(state: State<'_, AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::drop_commit(&path, &hash)).await
}

#[tauri::command]
async fn push_upto(
    state: State<'_, AppState>,
    remote: String,
    branch: String,
    hash: String,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::push_upto(&path, &remote, &branch, &hash)).await
}

#[tauri::command]
async fn commit_diff(
    state: State<'_, AppState>,
    from: String,
    to: String,
    repo: Option<String>,
) -> Result<FileDiff, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::commit_diff(&path, &from, &to)).await
}

#[tauri::command]
async fn fixup_commit(state: State<'_, AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::fixup_commit(&path, &hash)).await
}

#[tauri::command]
async fn squash_commit(state: State<'_, AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::squash_commit(&path, &hash)).await
}

#[tauri::command]
async fn rebase_plan(
    state: State<'_, AppState>,
    upstream: String,
    items: Vec<git::RebaseItem>,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::rebase_plan(&path, &upstream, &items)).await
}

#[tauri::command]
async fn rename_branch(
    state: State<'_, AppState>,
    old: String,
    new: String,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::rename_branch(&path, &old, &new)).await
}

#[tauri::command]
async fn set_upstream(
    state: State<'_, AppState>,
    local: String,
    upstream: String,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::set_upstream(&path, &local, &upstream)).await
}

#[tauri::command]
async fn update_branch(state: State<'_, AppState>, name: String, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::update_branch(&path, &name)).await
}

#[tauri::command]
async fn delete_branch(
    state: State<'_, AppState>,
    name: String,
    force: bool,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::delete_branch(&path, &name, force)).await
}

#[tauri::command]
async fn delete_tag(state: State<'_, AppState>, name: String, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::delete_tag(&path, &name)).await
}

#[tauri::command]
async fn push_tag(
    state: State<'_, AppState>,
    remote: String,
    name: String,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::push_tag(&path, &remote, &name)).await
}

#[tauri::command]
async fn remote_details(state: State<'_, AppState>, repo: Option<String>) -> Result<Vec<RemoteDetail>, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::remote_details(&path)).await
}

#[tauri::command]
async fn add_remote(
    state: State<'_, AppState>,
    name: String,
    url: String,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::add_remote(&path, &name, &url)).await
}

#[tauri::command]
async fn remove_remote(state: State<'_, AppState>, name: String, repo: Option<String>) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::remove_remote(&path, &name)).await
}

#[tauri::command]
async fn set_remote_url(
    state: State<'_, AppState>,
    name: String,
    url: String,
    repo: Option<String>,
) -> Result<String, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::set_remote_url(&path, &name, &url)).await
}

#[tauri::command]
async fn apply_patch(state: State<'_, AppState>, path: String, repo: Option<String>) -> Result<String, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::apply_patch(&repo, &path)).await
}

#[tauri::command]
async fn add_worktree(
    state: State<'_, AppState>,
    path: String,
    branch: String,
    create_branch: bool,
    repo: Option<String>,
) -> Result<String, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::add_worktree(&repo, &path, &branch, create_branch)).await
}

#[tauri::command]
async fn remove_worktree(state: State<'_, AppState>, path: String, repo: Option<String>) -> Result<String, String> {
    let repo = repo_dir(&state, repo)?;
    run_blocking(move || git::remove_worktree(&repo, &path)).await
}

#[tauri::command]
async fn working_tree_diff(
    state: State<'_, AppState>,
    rev: String,
    repo: Option<String>,
) -> Result<FileDiff, String> {
    let path = repo_dir(&state, repo)?;
    run_blocking(move || git::working_tree_diff(&path, &rev)).await
}

#[tauri::command]
fn start_terminal(
    app: AppHandle,
    hub: State<term::TerminalHub>,
    id: String,
    cwd: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    term::start(&*hub, app, id, cwd, cols, rows)
}

#[tauri::command]
fn write_terminal(hub: State<term::TerminalHub>, id: String, data: String) -> Result<(), String> {
    term::write(&*hub, &id, &data)
}

#[tauri::command]
fn resize_terminal(
    hub: State<term::TerminalHub>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    term::resize(&*hub, &id, cols, rows)
}

#[tauri::command]
fn close_terminal(hub: State<term::TerminalHub>, id: String) -> Result<(), String> {
    term::close(&*hub, &id)
}

#[tauri::command]
async fn check_update() -> Result<updater::UpdateInfo, String> {
    run_blocking(updater::check_update).await
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<String, String> {
    run_blocking(move || updater::install_update(app)).await
}

#[tauri::command]
fn app_version() -> String {
    updater::current_version()
}

#[tauri::command]
fn watch_repo(app: AppHandle, hub: State<watch::WatchHub>, path: String) -> Result<(), String> {
    watch::watch_repo(&hub, app, path)
}

#[tauri::command]
fn unwatch_repo(hub: State<watch::WatchHub>, path: String) -> Result<(), String> {
    watch::unwatch_repo(&hub, path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .manage(AppStore::open())
        .manage(term::TerminalHub::new())
        .manage(watch::WatchHub::default())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let title = format!("Rebased {}", updater::current_version());
                let _ = window.set_title(&title);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_repo,
            get_repo,
            get_status,
            get_log,
            get_commit_files,
            get_branches,
            get_recent_branches,
            stage_file,
            unstage_file,
            commit_changes,
            checkout_branch,
            fetch_remote,
            pull_remote,
            push_remote,
            get_remotes,
            push_to,
            get_merge_preview,
            get_merge_file,
            compare_merge,
            merge_branch,
            merge_abort,
            get_merge_state,
            merge_continue,
            resolve_conflict,
            write_resolved,
            get_conflict_file,
            get_working_diff,
            generate_commit_message,
            get_ai_config,
            save_ai_config,
            test_ai_config,
            get_recent_projects,
            record_recent_project,
            remove_recent_project,
            get_session,
            save_session,
            get_app_setting,
            set_app_setting,
            cherry_pick,
            checkout_revision,
            reset_to,
            revert_commit,
            undo_commit,
            create_branch_at,
            create_tag_at,
            format_patch,
            write_text_file,
            list_worktree,
            abs_worktree_path,
            reveal_in_os,
            open_in_os,
            get_file_blame,
            preview_file,
            save_worktree_file,
            copy_worktree,
            copy_worktree_to_clipboard,
            rename_worktree,
            delete_worktree,
            drop_commit,
            push_upto,
            commit_diff,
            fixup_commit,
            squash_commit,
            rebase_plan,
            rename_branch,
            set_upstream,
            update_branch,
            delete_branch,
            delete_tag,
            push_tag,
            remote_details,
            add_remote,
            remove_remote,
            set_remote_url,
            apply_patch,
            add_worktree,
            remove_worktree,
            working_tree_diff,
            start_terminal,
            write_terminal,
            resize_terminal,
            close_terminal,
            check_update,
            install_update,
            app_version,
            watch_repo,
            unwatch_repo
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
