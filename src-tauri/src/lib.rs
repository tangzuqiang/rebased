mod ai;
mod git;
mod store;

use git::{
    AppState, BranchInfo, FileDiff, FilePreview, GitCommit, GitFile, MergeFileContent, MergeOutcome,
    MergePreview, MergeState, RepoInfo, StatusPayload,
};
use std::path::PathBuf;
use store::{AppStore, RecentProject, SessionState};
use tauri::State;

fn repo_dir(state: &AppState, repo: Option<String>) -> Result<PathBuf, String> {
    state.resolve_repo(repo.as_deref())
}

#[tauri::command]
fn open_repo(state: State<AppState>, path: String) -> Result<RepoInfo, String> {
    git::open_repo(&state, &path)
}

#[tauri::command]
fn get_repo(state: State<AppState>, repo: Option<String>) -> Result<RepoInfo, String> {
    git::repo_info(&repo_dir(&state, repo)?)
}

#[tauri::command]
fn get_status(state: State<AppState>, repo: Option<String>) -> Result<StatusPayload, String> {
    git::status(&repo_dir(&state, repo)?)
}

#[tauri::command]
fn get_log(
    state: State<AppState>,
    limit: Option<u32>,
    repo: Option<String>,
    refs: Option<Vec<String>>,
) -> Result<Vec<GitCommit>, String> {
    git::log(
        &repo_dir(&state, repo)?,
        limit.unwrap_or(200),
        &refs.unwrap_or_default(),
    )
}

#[tauri::command]
fn get_commit_files(state: State<AppState>, hash: String, repo: Option<String>) -> Result<Vec<GitFile>, String> {
    git::commit_files(&repo_dir(&state, repo)?, &hash)
}

#[tauri::command]
fn get_branches(state: State<AppState>, repo: Option<String>) -> Result<Vec<BranchInfo>, String> {
    git::branches(&repo_dir(&state, repo)?)
}

#[tauri::command]
fn stage_file(state: State<AppState>, path: String, repo: Option<String>) -> Result<(), String> {
    git::stage(&repo_dir(&state, repo)?, &path)
}

#[tauri::command]
fn unstage_file(state: State<AppState>, path: String, repo: Option<String>) -> Result<(), String> {
    git::unstage(&repo_dir(&state, repo)?, &path)
}

#[tauri::command]
fn commit_changes(
    state: State<AppState>,
    message: String,
    amend: bool,
    paths: Vec<String>,
    repo: Option<String>,
) -> Result<(), String> {
    git::commit(&repo_dir(&state, repo)?, &message, amend, &paths)
}

#[tauri::command]
fn checkout_branch(state: State<AppState>, name: String, repo: Option<String>) -> Result<(), String> {
    git::checkout(&repo_dir(&state, repo)?, &name)
}

#[tauri::command]
fn fetch_remote(state: State<AppState>, repo: Option<String>) -> Result<String, String> {
    git::fetch(&repo_dir(&state, repo)?)
}

#[tauri::command]
fn pull_remote(state: State<AppState>, repo: Option<String>) -> Result<String, String> {
    git::pull(&repo_dir(&state, repo)?)
}

#[tauri::command]
fn push_remote(state: State<AppState>, repo: Option<String>) -> Result<String, String> {
    git::push(&repo_dir(&state, repo)?)
}

#[tauri::command]
fn get_remotes(state: State<AppState>, repo: Option<String>) -> Result<Vec<String>, String> {
    git::remotes(&repo_dir(&state, repo)?)
}

#[tauri::command]
fn push_to(
    state: State<AppState>,
    remote: String,
    branch: String,
    repo: Option<String>,
) -> Result<String, String> {
    git::push_to(&repo_dir(&state, repo)?, &remote, &branch)
}

#[tauri::command]
fn get_merge_preview(
    state: State<AppState>,
    theirs: String,
    repo: Option<String>,
) -> Result<MergePreview, String> {
    git::merge_preview(&repo_dir(&state, repo)?, &theirs)
}

#[tauri::command]
fn get_merge_file(
    state: State<AppState>,
    theirs: String,
    path: String,
    repo: Option<String>,
) -> Result<MergeFileContent, String> {
    git::merge_file(&repo_dir(&state, repo)?, &theirs, &path)
}

#[tauri::command]
fn merge_branch(state: State<AppState>, theirs: String, repo: Option<String>) -> Result<MergeOutcome, String> {
    git::merge_branch(&repo_dir(&state, repo)?, &theirs)
}

#[tauri::command]
fn merge_abort(state: State<AppState>, repo: Option<String>) -> Result<String, String> {
    git::merge_abort(&repo_dir(&state, repo)?)
}

#[tauri::command]
fn get_merge_state(state: State<AppState>, repo: Option<String>) -> Result<MergeState, String> {
    git::merge_state(&repo_dir(&state, repo)?)
}

#[tauri::command]
fn merge_continue(state: State<AppState>, repo: Option<String>) -> Result<String, String> {
    git::merge_continue(&repo_dir(&state, repo)?)
}

#[tauri::command]
fn resolve_conflict(
    state: State<AppState>,
    path: String,
    side: String,
    repo: Option<String>,
) -> Result<MergeState, String> {
    git::resolve_conflict(&repo_dir(&state, repo)?, &path, &side)
}

#[tauri::command]
fn write_resolved(
    state: State<AppState>,
    path: String,
    content: String,
    repo: Option<String>,
) -> Result<MergeState, String> {
    git::write_resolved(&repo_dir(&state, repo)?, &path, &content)
}

#[tauri::command]
fn get_conflict_file(
    state: State<AppState>,
    path: String,
    repo: Option<String>,
) -> Result<MergeFileContent, String> {
    git::conflict_file(&repo_dir(&state, repo)?, &path)
}

#[tauri::command]
fn get_working_diff(
    state: State<AppState>,
    path: String,
    untracked: bool,
    repo: Option<String>,
) -> Result<FileDiff, String> {
    git::working_diff(&repo_dir(&state, repo)?, &path, untracked)
}

#[tauri::command]
fn generate_commit_message(
    state: State<AppState>,
    store: State<AppStore>,
    paths: Vec<ai::DiffPath>,
    repo: Option<String>,
) -> Result<ai::GeneratedMessage, String> {
    ai::generate_commit_message(&state, &store, &paths, repo.as_deref())
}

#[tauri::command]
fn get_ai_config(store: State<AppStore>) -> ai::AiConfig {
    ai::get_ai_config(&store)
}

#[tauri::command]
fn save_ai_config(store: State<AppStore>, config: ai::AiConfig) -> Result<ai::AiConfig, String> {
    ai::save_ai_config(&store, config)
}

#[tauri::command]
fn test_ai_config(store: State<AppStore>) -> Result<String, String> {
    ai::test_ai_config(&store)
}

#[tauri::command]
fn get_recent_projects(store: State<AppStore>) -> Result<Vec<RecentProject>, String> {
    store.recent_projects()
}

#[tauri::command]
fn record_recent_project(
    store: State<AppStore>,
    path: String,
    name: String,
) -> Result<Vec<RecentProject>, String> {
    store.touch_recent(&path, &name)
}

#[tauri::command]
fn remove_recent_project(store: State<AppStore>, path: String) -> Result<Vec<RecentProject>, String> {
    store.remove_recent(&path)
}

#[tauri::command]
fn get_session(store: State<AppStore>) -> Result<SessionState, String> {
    store.get_session()
}

#[tauri::command]
fn save_session(store: State<AppStore>, session: SessionState) -> Result<SessionState, String> {
    store.save_session(session)
}

#[tauri::command]
fn get_app_setting(store: State<AppStore>, key: String) -> Result<Option<String>, String> {
    store.get_setting(&key)
}

#[tauri::command]
fn set_app_setting(store: State<AppStore>, key: String, value: String) -> Result<(), String> {
    store.set_setting(&key, &value)
}

#[tauri::command]
fn cherry_pick(state: State<AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    git::cherry_pick(&repo_dir(&state, repo)?, &hash)
}

#[tauri::command]
fn checkout_revision(state: State<AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    git::checkout_revision(&repo_dir(&state, repo)?, &hash)
}

#[tauri::command]
fn reset_to(
    state: State<AppState>,
    hash: String,
    mode: String,
    repo: Option<String>,
) -> Result<String, String> {
    git::reset_to(&repo_dir(&state, repo)?, &hash, &mode)
}

#[tauri::command]
fn revert_commit(state: State<AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    git::revert_commit(&repo_dir(&state, repo)?, &hash)
}

#[tauri::command]
fn undo_commit(state: State<AppState>, repo: Option<String>) -> Result<String, String> {
    git::undo_commit(&repo_dir(&state, repo)?)
}

#[tauri::command]
fn create_branch_at(
    state: State<AppState>,
    name: String,
    hash: String,
    checkout: bool,
    repo: Option<String>,
) -> Result<String, String> {
    git::create_branch_at(&repo_dir(&state, repo)?, &name, &hash, checkout)
}

#[tauri::command]
fn create_tag_at(
    state: State<AppState>,
    name: String,
    hash: String,
    repo: Option<String>,
) -> Result<String, String> {
    git::create_tag_at(&repo_dir(&state, repo)?, &name, &hash)
}

#[tauri::command]
fn format_patch(state: State<AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    git::format_patch(&repo_dir(&state, repo)?, &hash)
}

#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), String> {
    git::write_text_file(&path, &content)
}

#[tauri::command]
fn list_worktree(state: State<AppState>, repo: Option<String>) -> Result<Vec<String>, String> {
    git::list_worktree(&repo_dir(&state, repo)?)
}

#[tauri::command]
fn preview_file(
    state: State<AppState>,
    path: String,
    repo: Option<String>,
) -> Result<FilePreview, String> {
    git::preview_file(&repo_dir(&state, repo)?, &path)
}

#[tauri::command]
fn drop_commit(state: State<AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    git::drop_commit(&repo_dir(&state, repo)?, &hash)
}

#[tauri::command]
fn push_upto(
    state: State<AppState>,
    remote: String,
    branch: String,
    hash: String,
    repo: Option<String>,
) -> Result<String, String> {
    git::push_upto(&repo_dir(&state, repo)?, &remote, &branch, &hash)
}

#[tauri::command]
fn commit_diff(
    state: State<AppState>,
    from: String,
    to: String,
    repo: Option<String>,
) -> Result<FileDiff, String> {
    git::commit_diff(&repo_dir(&state, repo)?, &from, &to)
}

#[tauri::command]
fn fixup_commit(state: State<AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    git::fixup_commit(&repo_dir(&state, repo)?, &hash)
}

#[tauri::command]
fn squash_commit(state: State<AppState>, hash: String, repo: Option<String>) -> Result<String, String> {
    git::squash_commit(&repo_dir(&state, repo)?, &hash)
}

#[tauri::command]
fn rebase_plan(
    state: State<AppState>,
    upstream: String,
    items: Vec<git::RebaseItem>,
    repo: Option<String>,
) -> Result<String, String> {
    git::rebase_plan(&repo_dir(&state, repo)?, &upstream, &items)
}

#[tauri::command]
fn rename_branch(
    state: State<AppState>,
    old: String,
    new: String,
    repo: Option<String>,
) -> Result<String, String> {
    git::rename_branch(&repo_dir(&state, repo)?, &old, &new)
}

#[tauri::command]
fn set_upstream(
    state: State<AppState>,
    local: String,
    upstream: String,
    repo: Option<String>,
) -> Result<String, String> {
    git::set_upstream(&repo_dir(&state, repo)?, &local, &upstream)
}

#[tauri::command]
fn update_branch(state: State<AppState>, name: String, repo: Option<String>) -> Result<String, String> {
    git::update_branch(&repo_dir(&state, repo)?, &name)
}

#[tauri::command]
fn working_tree_diff(state: State<AppState>, rev: String, repo: Option<String>) -> Result<FileDiff, String> {
    git::working_tree_diff(&repo_dir(&state, repo)?, &rev)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .manage(AppStore::open())
        .invoke_handler(tauri::generate_handler![
            open_repo,
            get_repo,
            get_status,
            get_log,
            get_commit_files,
            get_branches,
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
            preview_file,
            drop_commit,
            push_upto,
            commit_diff,
            fixup_commit,
            squash_commit,
            rebase_plan,
            rename_branch,
            set_upstream,
            update_branch,
            working_tree_diff
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
