import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AiConfig,
  BranchInfo,
  FileDiff,
  FilePreview,
  GeneratedMessage,
  GitCommit,
  GitFile,
  MergeCompare,
  MergeFileContent,
  MergeOutcome,
  MergePreview,
  MergeState,
  RecentProject,
  RebaseItem,
  RepoInfo,
  SessionState,
  StatusPayload,
  UpdateInfo,
} from "./types";

function withRepo(repo: string | undefined, extra: Record<string, unknown> = {}) {
  return repo ? { ...extra, repo } : extra;
}

export async function pickAndOpenRepo(): Promise<RepoInfo | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "打开 Git 仓库",
  });
  if (!selected || Array.isArray(selected)) {
    return null;
  }
  return openRepo(selected);
}

export function openRepo(path: string) {
  return invoke<RepoInfo>("open_repo", { path });
}

export function getRepo(repo?: string) {
  return invoke<RepoInfo>("get_repo", withRepo(repo));
}

export function getStatus(repo?: string) {
  return invoke<StatusPayload>("get_status", withRepo(repo));
}

export function getLog(repo?: string, limit = 200, refs: string[] = []) {
  return invoke<GitCommit[]>("get_log", withRepo(repo, { limit, refs }));
}

export function getCommitFiles(repo: string | undefined, hash: string) {
  return invoke<GitFile[]>("get_commit_files", withRepo(repo, { hash }));
}

export function getBranches(repo?: string) {
  return invoke<BranchInfo[]>("get_branches", withRepo(repo));
}

export function getRecentBranches(repo?: string, limit = 4) {
  return invoke<string[]>("get_recent_branches", withRepo(repo, { limit }));
}

export function stageFile(repo: string | undefined, path: string) {
  return invoke<void>("stage_file", withRepo(repo, { path }));
}

export function unstageFile(repo: string | undefined, path: string) {
  return invoke<void>("unstage_file", withRepo(repo, { path }));
}

export function commitChanges(
  repo: string | undefined,
  message: string,
  amend: boolean,
  paths: string[],
) {
  return invoke<void>("commit_changes", withRepo(repo, { message, amend, paths }));
}

export function checkoutBranch(repo: string | undefined, name: string) {
  return invoke<void>("checkout_branch", withRepo(repo, { name }));
}

export function fetchRemote(repo?: string) {
  return invoke<string>("fetch_remote", withRepo(repo));
}

export function pullRemote(repo?: string) {
  return invoke<string>("pull_remote", withRepo(repo));
}

export function pushRemote(repo?: string) {
  return invoke<string>("push_remote", withRepo(repo));
}

export function getRemotes(repo?: string) {
  return invoke<string[]>("get_remotes", withRepo(repo));
}

export function pushTo(repo: string | undefined, remote: string, branch: string) {
  return invoke<string>("push_to", withRepo(repo, { remote, branch }));
}

export function getMergePreview(repo: string | undefined, theirs: string) {
  return invoke<MergePreview>("get_merge_preview", withRepo(repo, { theirs }));
}

export function getMergeFile(repo: string | undefined, theirs: string, path: string) {
  return invoke<MergeFileContent>("get_merge_file", withRepo(repo, { theirs, path }));
}

export function mergeBranch(repo: string | undefined, theirs: string) {
  return invoke<MergeOutcome>("merge_branch", withRepo(repo, { theirs }));
}

export function compareMerge(repo: string | undefined, theirs: string) {
  return invoke<MergeCompare>("compare_merge", withRepo(repo, { theirs }));
}

export function mergeAbort(repo?: string) {
  return invoke<string>("merge_abort", withRepo(repo));
}

export function getMergeState(repo?: string) {
  return invoke<MergeState>("get_merge_state", withRepo(repo));
}

export function mergeContinue(repo?: string) {
  return invoke<string>("merge_continue", withRepo(repo));
}

export function resolveConflict(repo: string | undefined, path: string, side: "ours" | "theirs") {
  return invoke<MergeState>("resolve_conflict", withRepo(repo, { path, side }));
}

export function writeResolved(repo: string | undefined, path: string, content: string) {
  return invoke<MergeState>("write_resolved", withRepo(repo, { path, content }));
}

export function getConflictFile(repo: string | undefined, path: string) {
  return invoke<MergeFileContent>("get_conflict_file", withRepo(repo, { path }));
}

export function getWorkingDiff(repo: string | undefined, path: string, untracked: boolean) {
  return invoke<FileDiff>("get_working_diff", withRepo(repo, { path, untracked }));
}

export function generateCommitMessage(
  repo: string | undefined,
  paths: Array<{ path: string; untracked: boolean }>,
) {
  return invoke<GeneratedMessage>("generate_commit_message", withRepo(repo, { paths }));
}

export function testAiConfig() {
  return invoke<string>("test_ai_config");
}

export function getAiConfig() {
  return invoke<AiConfig>("get_ai_config");
}

export function saveAiConfig(config: AiConfig) {
  return invoke<AiConfig>("save_ai_config", { config });
}

export function getRecentProjects() {
  return invoke<RecentProject[]>("get_recent_projects");
}

export function recordRecentProject(path: string, name: string) {
  return invoke<RecentProject[]>("record_recent_project", { path, name });
}

export function removeRecentProject(path: string) {
  return invoke<RecentProject[]>("remove_recent_project", { path });
}

export function getSession() {
  return invoke<SessionState>("get_session");
}

export function saveSession(session: SessionState) {
  return invoke<SessionState>("save_session", { session });
}

export function getAppSetting(key: string) {
  return invoke<string | null>("get_app_setting", { key });
}

export function setAppSetting(key: string, value: string) {
  return invoke<void>("set_app_setting", { key, value });
}

export function cherryPick(repo: string | undefined, hash: string) {
  return invoke<string>("cherry_pick", withRepo(repo, { hash }));
}

export function checkoutRevision(repo: string | undefined, hash: string) {
  return invoke<string>("checkout_revision", withRepo(repo, { hash }));
}

export function resetTo(repo: string | undefined, hash: string, mode: "soft" | "mixed" | "hard") {
  return invoke<string>("reset_to", withRepo(repo, { hash, mode }));
}

export function revertCommit(repo: string | undefined, hash: string) {
  return invoke<string>("revert_commit", withRepo(repo, { hash }));
}

export function undoCommit(repo?: string) {
  return invoke<string>("undo_commit", withRepo(repo));
}

export function createBranchAt(repo: string | undefined, name: string, hash: string, checkout: boolean) {
  return invoke<string>("create_branch_at", withRepo(repo, { name, hash, checkout }));
}

export function createTagAt(repo: string | undefined, name: string, hash: string) {
  return invoke<string>("create_tag_at", withRepo(repo, { name, hash }));
}

export function formatPatch(repo: string | undefined, hash: string) {
  return invoke<string>("format_patch", withRepo(repo, { hash }));
}

export function writeTextFile(path: string, content: string) {
  return invoke<void>("write_text_file", { path, content });
}

export function dropCommit(repo: string | undefined, hash: string) {
  return invoke<string>("drop_commit", withRepo(repo, { hash }));
}

export function pushUpto(repo: string | undefined, remote: string, branch: string, hash: string) {
  return invoke<string>("push_upto", withRepo(repo, { remote, branch, hash }));
}

export function commitDiff(repo: string | undefined, from: string, to: string) {
  return invoke<FileDiff>("commit_diff", withRepo(repo, { from, to }));
}

export function fixupCommit(repo: string | undefined, hash: string) {
  return invoke<string>("fixup_commit", withRepo(repo, { hash }));
}

export function squashCommit(repo: string | undefined, hash: string) {
  return invoke<string>("squash_commit", withRepo(repo, { hash }));
}

export function rebasePlan(repo: string | undefined, upstream: string, items: RebaseItem[]) {
  return invoke<string>("rebase_plan", withRepo(repo, { upstream, items }));
}

export function renameBranch(repo: string | undefined, old: string, newName: string) {
  return invoke<string>("rename_branch", withRepo(repo, { old, new: newName }));
}

export function setUpstream(repo: string | undefined, local: string, upstream: string) {
  return invoke<string>("set_upstream", withRepo(repo, { local, upstream }));
}

export function updateBranch(repo: string | undefined, name: string) {
  return invoke<string>("update_branch", withRepo(repo, { name }));
}

export function workingTreeDiff(repo: string | undefined, rev: string) {
  return invoke<FileDiff>("working_tree_diff", withRepo(repo, { rev }));
}

export function listWorktree(repo?: string) {
  return invoke<string[]>("list_worktree", withRepo(repo));
}

export function absWorktreePath(repo: string | undefined, path: string) {
  return invoke<string>("abs_worktree_path", withRepo(repo, { path }));
}

export function revealInOs(repo: string | undefined, path: string) {
  return invoke<void>("reveal_in_os", withRepo(repo, { path }));
}

export function openInOs(repo: string | undefined, path: string) {
  return invoke<void>("open_in_os", withRepo(repo, { path }));
}

export function previewFile(repo: string | undefined, path: string) {
  return invoke<FilePreview>("preview_file", withRepo(repo, { path }));
}

export function saveWorktreeFile(repo: string | undefined, path: string, content: string) {
  return invoke<FilePreview>("save_worktree_file", withRepo(repo, { path, content }));
}

export function startTerminal(id: string, cwd: string, cols: number, rows: number) {
  return invoke<void>("start_terminal", { id, cwd, cols, rows });
}

export function writeTerminal(id: string, data: string) {
  return invoke<void>("write_terminal", { id, data });
}

export function resizeTerminal(id: string, cols: number, rows: number) {
  return invoke<void>("resize_terminal", { id, cols, rows });
}

export function closeTerminal(id: string) {
  return invoke<void>("close_terminal", { id });
}

export function checkUpdate() {
  return invoke<UpdateInfo>("check_update");
}

export function installUpdate() {
  return invoke<string>("install_update");
}

export function appVersion() {
  return invoke<string>("app_version");
}
