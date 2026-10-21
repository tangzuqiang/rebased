use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

pub struct AppState {
    pub repo: Mutex<Option<PathBuf>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            repo: Mutex::new(None),
        }
    }

    pub fn repo_path(&self) -> Result<PathBuf, String> {
        self.repo
            .lock()
            .map_err(|_| "状态锁失败".to_string())?
            .clone()
            .ok_or_else(|| "尚未打开仓库".to_string())
    }

    pub fn resolve_repo(&self, repo: Option<&str>) -> Result<PathBuf, String> {
        match repo {
            Some(path) if !path.trim().is_empty() => Ok(PathBuf::from(path.trim())),
            _ => self.repo_path(),
        }
    }

    pub fn set_repo(&self, repo: PathBuf) -> Result<(), String> {
        let mut guard = self.repo.lock().map_err(|_| "状态锁失败".to_string())?;
        *guard = Some(repo);
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub path: String,
    pub name: String,
    pub branch: String,
    pub detached: bool,
    pub ahead: u32,
    pub behind: u32,
    pub has_remote: bool,
    pub head: String,
    pub user_name: String,
    pub is_git: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GitFile {
    pub path: String,
    pub status: String,
    pub staged: bool,
    pub untracked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommit {
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub author: String,
    pub date: String,
    pub timestamp: i64,
    pub refs: Vec<String>,
    pub parents: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub current: bool,
    pub remote: bool,
    pub kind: String,
    pub upstream: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub changes: Vec<GitFile>,
    pub unversioned: Vec<GitFile>,
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String, String> {
    with_index_lock_retry(repo, || exec_git(repo, args))
}

fn is_index_lock_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("index.lock") && (lower.contains("file exists") || lower.contains("unable to create"))
}

fn index_lock_path(repo: &Path) -> PathBuf {
    let git = repo.join(".git");
    if git.is_file() {
        if let Ok(text) = std::fs::read_to_string(&git) {
            if let Some(dir) = text.strip_prefix("gitdir:") {
                return PathBuf::from(dir.trim()).join("index.lock");
            }
        }
    }
    git.join("index.lock")
}

fn git_process_running() -> bool {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("tasklist");
        cmd.args(["/FI", "IMAGENAME eq git.exe", "/FO", "CSV", "/NH"]);
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        cmd.output()
            .map(|out| decode(&out.stdout).to_ascii_lowercase().contains("git.exe"))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("pgrep")
            .args(["-x", "git"])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

fn clear_stale_index_lock(repo: &Path) -> bool {
    let lock = index_lock_path(repo);
    if !lock.is_file() || git_process_running() {
        return false;
    }
    std::fs::remove_file(&lock).is_ok()
}

fn with_index_lock_retry(
    repo: &Path,
    mut exec: impl FnMut() -> Result<(bool, String, String), String>,
) -> Result<String, String> {
    let mut last = String::new();
    for attempt in 0..5 {
        let (ok, stdout, stderr) = exec()?;
        if ok {
            return Ok(stdout);
        }
        last = if !stderr.trim().is_empty() { stderr } else { stdout };
        if !is_index_lock_error(&last) {
            return Err(localize_git_error(last.trim()));
        }
        if attempt < 3 {
            std::thread::sleep(Duration::from_millis(180));
            continue;
        }
        if attempt == 3 {
            let _ = clear_stale_index_lock(repo);
        }
    }
    Err(localize_git_error(last.trim()))
}

fn exec_git(repo: &Path, args: &[&str]) -> Result<(bool, String, String), String> {
    let (ok, stdout, stderr) = exec_git_raw(repo, args)?;
    Ok((ok, decode(&stdout), decode(&stderr)))
}

fn exec_git_raw(repo: &Path, args: &[&str]) -> Result<(bool, Vec<u8>, Vec<u8>), String> {
    let mut cmd = Command::new("git");
    cmd.arg("-c")
        .arg("core.quotepath=false")
        .arg("-c")
        .arg("i18n.logOutputEncoding=utf-8")
        .arg("-c")
        .arg(format!("core.editor={}", non_interactive_editor()))
        .arg("-c")
        .arg(format!("sequence.editor={}", non_interactive_editor()))
        .args(args)
        .current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_EDITOR", non_interactive_editor())
        .env("GIT_SEQUENCE_EDITOR", non_interactive_editor())
        .env("GIT_MERGE_AUTOEDIT", "no")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("无法执行 git，请确认已安装并加入 PATH：{e}"))?;
    Ok((output.status.success(), output.stdout, output.stderr))
}

fn decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn repo_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repository")
        .to_string()
}

fn has_git(path: &Path) -> bool {
    run_git(path, &["rev-parse", "--git-dir"]).is_ok()
}

fn folder_info(path: &Path) -> RepoInfo {
    RepoInfo {
        path: path.to_string_lossy().into_owned(),
        name: repo_name(path),
        branch: String::new(),
        detached: false,
        ahead: 0,
        behind: 0,
        has_remote: false,
        head: String::new(),
        user_name: String::new(),
        is_git: false,
    }
}

pub fn discover_repo(path: &str) -> Result<(PathBuf, RepoInfo), String> {
    let input = PathBuf::from(path.trim());
    if !input.exists() {
        return Err(format!("路径不存在：{path}"));
    }
    if !input.is_dir() {
        return Err(format!("请选择一个目录：{path}"));
    }
    Ok((input.clone(), repo_info(&input)?))
}

pub fn repo_info(repo: &Path) -> Result<RepoInfo, String> {
    if !has_git(repo) {
        return Ok(folder_info(repo));
    }

    let head_name = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let detached = head_name.as_deref() == Some("HEAD");

    let branch = match head_name.as_deref() {
        Some("HEAD") => run_git(repo, &["rev-parse", "--short", "HEAD"])
            .map(|s| format!("HEAD ({})", s.trim()))
            .unwrap_or_else(|_| "未提交".to_string()),
        Some(name) => name.to_string(),
        None => "未提交".to_string(),
    };

    let mut ahead = 0u32;
    let mut behind = 0u32;
    let mut has_remote = false;

    if let Ok(raw) = run_git(
        repo,
        &[
            "rev-list",
            "--left-right",
            "--count",
            "@{upstream}...HEAD",
        ],
    ) {
        has_remote = true;
        let parts: Vec<&str> = raw.split_whitespace().collect();
        if parts.len() >= 2 {
            behind = parts[0].parse().unwrap_or(0);
            ahead = parts[1].parse().unwrap_or(0);
        }
    } else if run_git(repo, &["remote"]).map(|s| !s.trim().is_empty()).unwrap_or(false) {
        has_remote = true;
    }

    let head = run_git(repo, &["rev-parse", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let user_name = run_git(repo, &["config", "user.name"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    Ok(RepoInfo {
        path: repo.to_string_lossy().into_owned(),
        name: repo_name(repo),
        branch,
        detached,
        ahead,
        behind,
        has_remote,
        head,
        user_name,
        is_git: true,
    })
}

pub fn status(repo: &Path) -> Result<StatusPayload, String> {
    let raw = run_git(repo, &["status", "--porcelain=v1", "-uall"])?;
    let mut changes = Vec::new();
    let mut unversioned = Vec::new();

    for line in raw.lines() {
        if line.len() < 3 {
            continue;
        }
        let x = line.as_bytes()[0] as char;
        let y = line.as_bytes()[1] as char;
        let rest = &line[3..];

        let path = if let Some((from, to)) = rest.split_once(" -> ") {
            if x == 'R' || y == 'R' || x == 'C' || y == 'C' {
                format!("{from} → {to}")
            } else {
                to.to_string()
            }
        } else {
            rest.to_string()
        };

        if x == '?' && y == '?' {
            unversioned.push(GitFile {
                path,
                status: "U".to_string(),
                staged: false,
                untracked: true,
            });
            continue;
        }

        if x != ' ' && x != '?' {
            changes.push(GitFile {
                path: path.clone(),
                status: status_letter(x),
                staged: true,
                untracked: false,
            });
        }
        if y != ' ' && y != '?' {
            changes.push(GitFile {
                path,
                status: status_letter(y),
                staged: false,
                untracked: false,
            });
        }
    }

    Ok(StatusPayload {
        changes,
        unversioned,
    })
}

fn status_letter(code: char) -> String {
    match code {
        'A' => "A",
        'D' => "D",
        'R' => "R",
        'C' => "C",
        'U' => "U",
        _ => "M",
    }
    .to_string()
}

pub fn log(repo: &Path, limit: u32, refs: &[String]) -> Result<Vec<GitCommit>, String> {
    let mut args = vec![
        "log".to_string(),
        format!("-n{limit}"),
        "--pretty=format:%H%x1f%h%x1f%s%x1f%an%x1f%at%x1f%D%x1f%P".to_string(),
    ];
    if refs.iter().any(|item| item == "--all" || item.eq_ignore_ascii_case("all")) {
        args.push("--all".to_string());
    } else if refs.is_empty() {
        args.push("HEAD".to_string());
    } else {
        args.extend(refs.iter().cloned());
    }
    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
    let raw = match run_git(repo, &args_ref) {
        Ok(value) => value,
        Err(err) if is_empty_history(&err) => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };

    let mut commits = Vec::new();
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\u{1f}').collect();
        if parts.len() < 7 {
            continue;
        }
        let refs = if parts[5].trim().is_empty() {
            Vec::new()
        } else {
            parts[5]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };
        let parents = if parts[6].trim().is_empty() {
            Vec::new()
        } else {
            parts[6]
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        };
        let timestamp = parts[4].parse::<i64>().unwrap_or(0);
        commits.push(GitCommit {
            hash: parts[0].to_string(),
            short_hash: parts[1].to_string(),
            message: parts[2].to_string(),
            author: parts[3].to_string(),
            date: format_unix(timestamp),
            timestamp,
            refs,
            parents,
        });
    }
    Ok(commits)
}

pub fn commit_message(repo: &Path, hash: &str) -> Result<String, String> {
    let text = run_git(repo, &["log", "-1", "--pretty=%B", hash])?;
    Ok(text.trim_end().to_string())
}

pub fn head_commit_message(repo: &Path) -> Result<String, String> {
    commit_message(repo, "HEAD")
}

pub fn commit_files(repo: &Path, hash: &str) -> Result<Vec<GitFile>, String> {
    let raw = run_git(
        repo,
        &[
            "show",
            "--name-status",
            "--pretty=format:",
            "--no-renames",
            hash,
        ],
    )?;
    let mut files = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let code = parts.next().unwrap_or("M");
        let path = parts.next().unwrap_or("").to_string();
        if path.is_empty() {
            continue;
        }
        files.push(GitFile {
            path,
            status: status_letter(code.chars().next().unwrap_or('M')),
            staged: false,
            untracked: false,
        });
    }
    Ok(files)
}

pub fn branches(repo: &Path) -> Result<Vec<BranchInfo>, String> {
    let raw = run_git(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)%09%(HEAD)%09%(refname)%09%(upstream:short)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    let mut list = Vec::new();
    for line in raw.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let name = parts[0].to_string();
        if name == "origin" || name.ends_with("/HEAD") {
            continue;
        }
        let remote = parts[2].starts_with("refs/remotes/");
        list.push(BranchInfo {
            current: parts[1] == "*",
            remote,
            kind: if remote { "remote" } else { "local" }.to_string(),
            name,
            upstream: parts.get(3).unwrap_or(&"").trim().to_string(),
        });
    }
    if let Ok(tags) = run_git(repo, &["for-each-ref", "--format=%(refname:short)", "refs/tags"]) {
        for name in tags.lines() {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            list.push(BranchInfo {
                current: false,
                remote: false,
                kind: "tag".to_string(),
                name: name.to_string(),
                upstream: String::new(),
            });
        }
    }
    Ok(list)
}

pub fn recent_branches(repo: &Path, limit: usize) -> Result<Vec<String>, String> {
    let cap = if limit == 0 { 4 } else { limit };
    let known: Vec<String> = match branches(repo) {
        Ok(items) => items
            .into_iter()
            .filter(|item| item.kind != "tag")
            .map(|item| item.name)
            .collect(),
        Err(_) => Vec::new(),
    };
    let raw = run_git(repo, &["reflog", "--format=%gs", "-n", "200"]).unwrap_or_default();
    let mut seen = Vec::new();
    let mut list = Vec::new();
    for line in raw.lines() {
        let Some(rest) = line.strip_prefix("checkout: moving from ") else {
            continue;
        };
        let Some((_, to)) = rest.split_once(" to ") else {
            continue;
        };
        let name = to.trim();
        if name.is_empty() || name == "HEAD" || seen.iter().any(|item| item == name) {
            continue;
        }
        if !known.is_empty() && !known.iter().any(|item| item == name) {
            continue;
        }
        seen.push(name.to_string());
        list.push(name.to_string());
        if list.len() >= cap {
            break;
        }
    }
    if list.is_empty() {
        let current = current_branch(repo);
        if !current.is_empty() && current != "HEAD" {
            list.push(current);
        }
    }
    Ok(list)
}

pub fn stage(repo: &Path, path: &str) -> Result<(), String> {
    let actual = display_to_path(path);
    run_git(repo, &["add", "--", &actual])?;
    Ok(())
}

pub fn unstage(repo: &Path, path: &str) -> Result<(), String> {
    let actual = display_to_path(path);
    run_git(repo, &["restore", "--staged", "--", &actual])?;
    Ok(())
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct WorktreePathEntry {
    pub path: String,
    pub untracked: bool,
}

pub fn discard_worktree_changes(repo: &Path, items: &[WorktreePathEntry]) -> Result<String, String> {
    if items.is_empty() {
        return Err("请至少选择一个文件".to_string());
    }

    let mut tracked = Vec::new();
    let mut untracked = Vec::new();
    for item in items {
        let actual = display_to_path(&item.path);
        if actual.trim().is_empty() {
            continue;
        }
        if item.untracked {
            untracked.push(actual);
        } else {
            tracked.push(actual);
        }
    }
    tracked.sort();
    tracked.dedup();
    untracked.sort();
    untracked.dedup();

    if tracked.is_empty() && untracked.is_empty() {
        return Err("没有可回滚的文件".to_string());
    }

    for path in &tracked {
        run_git(
            repo,
            &["restore", "--staged", "--worktree", "--source=HEAD", "--", path],
        )?;
    }
    for path in &untracked {
        run_git(repo, &["clean", "-fd", "--", path])?;
    }

    let count = tracked.len() + untracked.len();
    Ok(format!("已回滚 {count} 个文件"))
}

pub fn commit(repo: &Path, message: &str, amend: bool, include_paths: &[String]) -> Result<(), String> {
    if message.trim().is_empty() && !amend {
        return Err("请填写提交说明".to_string());
    }

    let paths: Vec<String> = include_paths.iter().map(|p| display_to_path(p)).collect();
    if !paths.is_empty() {
        let mut args = vec!["add".to_string(), "--".to_string()];
        args.extend(paths.iter().cloned());
        let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
        run_git(repo, &args_ref)?;
    }

    let mut args = vec!["commit".to_string()];
    if amend {
        args.push("--amend".to_string());
        if message.trim().is_empty() {
            args.push("--no-edit".to_string());
        } else {
            args.push("-m".to_string());
            args.push(message.to_string());
        }
    } else {
        args.push("-m".to_string());
        args.push(message.to_string());
    }
    if !paths.is_empty() {
        args.push("--".to_string());
        args.extend(paths);
    }
    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git(repo, &args_ref)?;
    Ok(())
}

fn ref_exists(repo: &Path, reference: &str) -> bool {
    exec_git(repo, &["show-ref", "--verify", "--quiet", reference])
        .map(|(ok, _, _)| ok)
        .unwrap_or(false)
}

fn remote_tracking_target(repo: &Path, branch: &str) -> Option<String> {
    let remote_ref = format!("refs/remotes/{branch}");
    if !ref_exists(repo, &remote_ref) {
        return None;
    }

    let remotes = run_git(repo, &["remote"]).ok()?;
    remotes
        .lines()
        .map(str::trim)
        .filter(|remote| !remote.is_empty())
        .filter_map(|remote| {
            branch
                .strip_prefix(&format!("{remote}/"))
                .map(|name| (remote.len(), name))
        })
        .filter(|(_, name)| !name.is_empty() && *name != "HEAD")
        .max_by_key(|(remote_len, _)| *remote_len)
        .map(|(_, name)| name.to_string())
}

pub fn checkout(repo: &Path, branch: &str) -> Result<(), String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Err("请选择要签出的分支".to_string());
    }

    // Prefer an exact local branch when local and remote display names happen to collide.
    if ref_exists(repo, &format!("refs/heads/{branch}")) {
        run_git(repo, &["checkout", branch])?;
        return Ok(());
    }

    // Checking out a remote-tracking ref directly detaches HEAD. Create the matching
    // local branch (or reuse it) and configure its upstream in the same Git command.
    if let Some(local) = remote_tracking_target(repo, branch) {
        if ref_exists(repo, &format!("refs/heads/{local}")) {
            run_git(repo, &["checkout", &local])?;
        } else {
            run_git(repo, &["checkout", "--track", "-b", &local, branch])?;
        }
        return Ok(());
    }

    run_git(repo, &["checkout", branch])?;
    Ok(())
}

pub fn fetch(repo: &Path) -> Result<String, String> {
    run_git(repo, &["fetch", "--all", "--prune"])
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullOutcome {
    pub status: String,
    pub message: String,
    pub upstream: String,
}

pub fn pull(repo: &Path) -> Result<PullOutcome, String> {
    let (ok, stdout, stderr) = exec_git(repo, &["pull", "--ff-only"])?;
    let combined = format!("{stdout}\n{stderr}");
    if ok {
        return Ok(PullOutcome {
            status: "ok".to_string(),
            message: combined.trim().to_string(),
            upstream: String::new(),
        });
    }

    let lower = combined.to_ascii_lowercase();
    if lower.contains("not possible to fast-forward") || lower.contains("divergent branches") {
        let branch = current_branch(repo);
        let upstream = branch_upstream(repo, &branch);
        if upstream.is_empty() {
            return Err("当前分支没有设置上游，无法合并远程更新".to_string());
        }
        return Ok(PullOutcome {
            status: "diverged".to_string(),
            message: "本地与远程均有新提交，需要确认后合并".to_string(),
            upstream,
        });
    }

    Err(localize_git_error(combined.trim()))
}

pub fn push(repo: &Path) -> Result<String, String> {
    run_git(repo, &["push"])
}

/// 批量同步的结果：ok 直接完成，merged 自动合并后完成，conflicts 合并冲突待处理。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    pub status: String,
    pub message: String,
    pub upstream: String,
    pub conflicts: Vec<String>,
}

fn sync_outcome(status: &str, message: String, upstream: String, conflicts: Vec<String>) -> SyncOutcome {
    SyncOutcome {
        status: status.to_string(),
        message,
        upstream,
        conflicts,
    }
}

/// 合并上游；产生冲突时保留合并状态交给用户处理。
fn merge_upstream(repo: &Path, upstream: &str) -> Result<Option<SyncOutcome>, String> {
    let outcome = merge_branch(repo, upstream)?;
    if outcome.status == "conflicts" {
        let conflicts: Vec<String> = outcome.state.files.into_iter().map(|file| file.path).collect();
        return Ok(Some(sync_outcome(
            "conflicts",
            format!("自动合并 {upstream} 产生 {} 个冲突文件，需要手动处理", conflicts.len()),
            upstream.to_string(),
            conflicts,
        )));
    }
    Ok(None)
}

fn current_upstream(repo: &Path) -> Result<String, String> {
    let upstream = branch_upstream(repo, &current_branch(repo));
    if upstream.is_empty() {
        Err("当前分支没有设置上游，无法自动合并远程更新".to_string())
    } else {
        Ok(upstream)
    }
}

/// 拉取：能快进就快进；与远程分叉时自动合并上游，只有冲突才停下。
pub fn sync_pull(repo: &Path) -> Result<SyncOutcome, String> {
    let (ok, stdout, stderr) = exec_git(repo, &["pull", "--ff-only"])?;
    let combined = format!("{stdout}\n{stderr}");
    if ok {
        return Ok(sync_outcome("ok", combined.trim().to_string(), String::new(), Vec::new()));
    }
    let lower = combined.to_ascii_lowercase();
    if !(lower.contains("not possible to fast-forward") || lower.contains("divergent branches")) {
        return Err(localize_git_error(combined.trim()));
    }
    let upstream = current_upstream(repo)?;
    if let Some(conflict) = merge_upstream(repo, &upstream)? {
        return Ok(conflict);
    }
    Ok(sync_outcome(
        "merged",
        format!("本地与远程分叉，已自动合并 {upstream}"),
        upstream,
        Vec::new(),
    ))
}

fn is_push_rejected(lower: &str) -> bool {
    lower.contains("rejected")
        && (lower.contains("non-fast-forward") || lower.contains("fetch first") || lower.contains("behind"))
}

pub fn sync_push(repo: &Path) -> Result<SyncOutcome, String> {
    sync_push_to(repo, "", "")
}

/// 推送：远程有新提交被拒绝时，先获取并自动合并目标分支再推送；有冲突则不推送。
/// remote 为空时推送到当前分支的上游。
pub fn sync_push_to(repo: &Path, remote: &str, branch: &str) -> Result<SyncOutcome, String> {
    let (remote, branch) = (remote.trim(), branch.trim());
    let args: Vec<&str> = match (remote.is_empty(), branch.is_empty()) {
        (true, _) => vec!["push"],
        (false, true) => vec!["push", remote],
        (false, false) => vec!["push", "-u", remote, branch],
    };
    let (ok, stdout, stderr) = exec_git(repo, &args)?;
    let combined = format!("{stdout}\n{stderr}");
    if ok {
        return Ok(sync_outcome("ok", combined.trim().to_string(), String::new(), Vec::new()));
    }
    if !is_push_rejected(&combined.to_ascii_lowercase()) {
        return Err(localize_git_error(combined.trim()));
    }
    let upstream = if !remote.is_empty() && !branch.is_empty() {
        format!("{remote}/{branch}")
    } else {
        current_upstream(repo)?
    };
    let fetch_remote = if remote.is_empty() {
        upstream.split('/').next().unwrap_or("origin").to_string()
    } else {
        remote.to_string()
    };
    run_git(repo, &["fetch", &fetch_remote])?;
    if let Some(conflict) = merge_upstream(repo, &upstream)? {
        return Ok(conflict);
    }
    let (ok, stdout, stderr) = exec_git(repo, &args)?;
    if !ok {
        return Err(localize_git_error(format!("{stdout}\n{stderr}").trim()));
    }
    Ok(sync_outcome(
        "merged",
        format!("远程有新提交，已自动合并 {upstream} 后推送"),
        upstream,
        Vec::new(),
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub binary: bool,
    pub patch: String,
}

pub fn working_diff(repo: &Path, path: &str, untracked: bool) -> Result<FileDiff, String> {
    let actual = display_to_path(path);
    if untracked {
        return untracked_diff(repo, &actual);
    }

    let mut patch = run_git(repo, &["diff", "-U3", "HEAD", "--", &actual]).unwrap_or_default();
    if patch.trim().is_empty() {
        let staged = run_git(repo, &["diff", "-U3", "--cached", "--", &actual]).unwrap_or_default();
        let work = run_git(repo, &["diff", "-U3", "--", &actual]).unwrap_or_default();
        patch = format!("{staged}{work}");
    }
    if patch.contains("Binary files") || patch.contains("二进制文件") {
        return Ok(FileDiff {
            path: actual,
            binary: true,
            patch: String::new(),
        });
    }
    Ok(FileDiff {
        path: actual,
        binary: false,
        patch,
    })
}

fn untracked_diff(repo: &Path, path: &str) -> Result<FileDiff, String> {
    let full = repo.join(path);
    let bytes = std::fs::read(&full).map_err(|e| format!("无法读取文件：{e}"))?;
    if bytes.contains(&0) {
        return Ok(FileDiff {
            path: path.to_string(),
            binary: true,
            patch: String::new(),
        });
    }
    let text = decode_text(&bytes).0;
    let lines: Vec<&str> = text.lines().collect();
    let mut patch = format!(
        "--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n",
        lines.len()
    );
    for line in lines {
        patch.push('+');
        patch.push_str(line);
        patch.push('\n');
    }
    Ok(FileDiff {
        path: path.to_string(),
        binary: false,
        patch,
    })
}

pub fn remotes(repo: &Path) -> Result<Vec<String>, String> {
    Ok(run_git(repo, &["remote"])?
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDetail {
    pub name: String,
    pub url: String,
}

pub fn remote_details(repo: &Path) -> Result<Vec<RemoteDetail>, String> {
    let names = remotes(repo)?;
    let mut out = Vec::new();
    for name in names {
        let url = run_git(repo, &["remote", "get-url", &name]).unwrap_or_default();
        out.push(RemoteDetail {
            name,
            url: url.trim().to_string(),
        });
    }
    Ok(out)
}

pub fn push_to(repo: &Path, remote: &str, branch: &str) -> Result<String, String> {
    if remote.trim().is_empty() {
        return push(repo);
    }
    if branch.trim().is_empty() {
        return run_git(repo, &["push", remote]);
    }
    run_git(repo, &["push", "-u", remote, branch])
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeFileInfo {
    pub path: String,
    pub ours_changed: bool,
    pub theirs_changed: bool,
    pub conflict: bool,
    pub ours_exists: bool,
    pub base_exists: bool,
    pub theirs_exists: bool,
    pub status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePreview {
    pub ours: String,
    pub theirs: String,
    pub base: String,
    pub base_short: String,
    pub files: Vec<MergeFileInfo>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlameLine {
    pub author: String,
    pub timestamp: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeFileContent {
    pub path: String,
    pub ours: String,
    pub base: String,
    pub theirs: String,
    pub binary: bool,
    pub too_large: bool,
    pub ours_blame: Vec<BlameLine>,
    pub base_blame: Vec<BlameLine>,
    pub theirs_blame: Vec<BlameLine>,
}

fn parse_blame(raw: &str) -> Vec<BlameLine> {
    let mut lines = Vec::new();
    let mut author = String::new();
    let mut timestamp = 0i64;
    for line in raw.lines() {
        if let Some(value) = line.strip_prefix("author ") {
            author = value.to_string();
        } else if let Some(value) = line.strip_prefix("author-time ") {
            timestamp = value.parse().unwrap_or(0);
        } else if line.starts_with('\t') {
            lines.push(BlameLine {
                author: author.clone(),
                timestamp,
            });
        }
    }
    lines
}

pub fn worktree_blame(repo: &Path, path: &str) -> Result<Vec<BlameLine>, String> {
    let (rel, _) = resolve_worktree_file(repo, path)?;
    let raw = run_git(repo, &["blame", "--line-porcelain", "-w", "--", &rel])
        .or_else(|_| run_git(repo, &["blame", "--line-porcelain", "-w", "HEAD", "--", &rel]))
        .unwrap_or_default();
    Ok(parse_blame(&raw))
}

fn blame_lines(repo: &Path, rev: &str, path: &str) -> Vec<BlameLine> {
    if rev.trim().is_empty() || path.trim().is_empty() {
        return Vec::new();
    }
    let raw = run_git(
        repo,
        &["blame", "--line-porcelain", "-w", rev, "--", path],
    )
    .unwrap_or_default();
    parse_blame(&raw)
}

pub fn merge_preview(repo: &Path, theirs: &str) -> Result<MergePreview, String> {
    if theirs.trim().is_empty() {
        return Err("请选择要合并的分支".to_string());
    }
    let ours = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    let base = run_git(repo, &["merge-base", "HEAD", theirs])?
        .trim()
        .to_string();
    if base.is_empty() {
        return Err("没有共同祖先，无法进行三端对比".to_string());
    }
    let base_short = run_git(repo, &["rev-parse", "--short", &base])?
        .trim()
        .to_string();

    let ours_map = name_status_map(repo, &base, "HEAD")?;
    let theirs_map = name_status_map(repo, &base, theirs)?;
    let mut paths: Vec<String> = ours_map.keys().chain(theirs_map.keys()).cloned().collect();
    paths.sort();
    paths.dedup();

    let mut files = Vec::new();
    for path in paths {
        let ours_changed = ours_map.contains_key(&path);
        let theirs_changed = theirs_map.contains_key(&path);
        let ours_exists = exists_at(repo, "HEAD", &path);
        let base_exists = exists_at(repo, &base, &path);
        let theirs_exists = exists_at(repo, theirs, &path);
        let status = if ours_changed && theirs_changed {
            "C".to_string()
        } else if !base_exists && (ours_exists || theirs_exists) {
            "A".to_string()
        } else if base_exists && !ours_exists && !theirs_exists {
            "D".to_string()
        } else {
            ours_map
                .get(&path)
                .or_else(|| theirs_map.get(&path))
                .cloned()
                .unwrap_or_else(|| "M".to_string())
        };
        files.push(MergeFileInfo {
            path,
            ours_changed,
            theirs_changed,
            conflict: ours_changed && theirs_changed,
            ours_exists,
            base_exists,
            theirs_exists,
            status,
        });
    }

    Ok(MergePreview {
        ours,
        theirs: theirs.to_string(),
        base,
        base_short,
        files,
    })
}

pub fn merge_file(repo: &Path, theirs: &str, path: &str) -> Result<MergeFileContent, String> {
    if merge_in_progress(repo) {
        return conflict_file(repo, path);
    }
    let base = run_git(repo, &["merge-base", "HEAD", theirs])?
        .trim()
        .to_string();
    let ours = show_blob(repo, "HEAD", path);
    let base_blob = show_blob(repo, &base, path);
    let theirs_blob = show_blob(repo, theirs, path);
    Ok(three_way_content(
        repo,
        path,
        [ours, base_blob, theirs_blob],
        ["HEAD", &base, theirs],
    ))
}

pub fn conflict_file(repo: &Path, path: &str) -> Result<MergeFileContent, String> {
    let ours = show_blob(repo, ":2", path).or_else(|| show_blob(repo, "HEAD", path));
    let base_blob = show_blob(repo, ":1", path);
    let theirs_blob = show_blob(repo, ":3", path);
    let theirs_rev = run_git(repo, &["rev-parse", "MERGE_HEAD"])
        .map(|item| item.trim().to_string())
        .unwrap_or_else(|_| "MERGE_HEAD".to_string());
    let base_rev = run_git(repo, &["merge-base", "HEAD", &theirs_rev])
        .map(|item| item.trim().to_string())
        .unwrap_or_default();
    Ok(three_way_content(
        repo,
        path,
        [ours, base_blob, theirs_blob],
        ["HEAD", &base_rev, &theirs_rev],
    ))
}

fn three_way_content(
    repo: &Path,
    path: &str,
    blobs: [Option<Vec<u8>>; 3],
    revs: [&str; 3],
) -> MergeFileContent {
    let binary = blobs.iter().flatten().any(|bytes| bytes.contains(&0));
    let too_large = blobs.iter().flatten().any(|bytes| bytes.len() > MAX_MERGE_BYTES);
    let readable = !binary && !too_large;
    let [ours, base, theirs] = blobs.map(|blob| match blob {
        Some(bytes) if readable => decode_text(&bytes).0,
        _ => String::new(),
    });
    let [ours_blame, base_blame, theirs_blame] = revs.map(|rev| {
        if readable && !rev.is_empty() {
            blame_lines(repo, rev, path)
        } else {
            Vec::new()
        }
    });
    MergeFileContent {
        path: path.to_string(),
        ours,
        base,
        theirs,
        binary,
        too_large,
        ours_blame,
        base_blame,
        theirs_blame,
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    pub path: String,
    pub ours_status: String,
    pub theirs_status: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MergeState {
    pub in_progress: bool,
    pub ours: String,
    pub theirs: String,
    pub files: Vec<ConflictFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOutcome {
    pub status: String,
    pub message: String,
    pub state: MergeState,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeCompare {
    pub can_merge: bool,
    pub message: String,
}

fn is_fatal_git(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("fatal") || lower.contains("bad revision") || lower.contains("unknown revision")
}

pub fn compare_merge(repo: &Path, theirs: &str) -> Result<MergeCompare, String> {
    if theirs.trim().is_empty() {
        return Ok(MergeCompare {
            can_merge: false,
            message: "请选择要合并的分支".to_string(),
        });
    }

    let (already, _, ancestor_err) =
        exec_git(repo, &["merge-base", "--is-ancestor", theirs, "HEAD"])?;
    if is_fatal_git(&ancestor_err) {
        return Err(localize_git_error(ancestor_err.trim()));
    }
    if already {
        return Ok(MergeCompare {
            can_merge: false,
            message: "该分支已合并，或与当前分支相同".to_string(),
        });
    }

    let (no_diff, _, diff_err) = exec_git(repo, &["diff", "--quiet", "HEAD", theirs])?;
    if is_fatal_git(&diff_err) {
        return Err(localize_git_error(diff_err.trim()));
    }
    if no_diff {
        return Ok(MergeCompare {
            can_merge: false,
            message: "两个分支代码相同，无需合并".to_string(),
        });
    }

    Ok(MergeCompare {
        can_merge: true,
        message: String::new(),
    })
}

pub fn merge_branch(repo: &Path, theirs: &str) -> Result<MergeOutcome, String> {
    if theirs.trim().is_empty() {
        return Err("请选择要合并的分支".to_string());
    }
    let (ok, stdout, stderr) = exec_git(repo, &["merge", "--no-edit", theirs])?;
    let combined = format!("{stdout}\n{stderr}");
    if merge_in_progress(repo) {
        return Ok(MergeOutcome {
            status: "conflicts".to_string(),
            message: "合并产生冲突，请处理冲突文件".to_string(),
            state: merge_state(repo)?,
        });
    }
    if !ok {
        return Err(localize_git_error(combined.trim()));
    }
    Ok(MergeOutcome {
        status: "ok".to_string(),
        message: if stdout.trim().is_empty() {
            "合并完成".to_string()
        } else {
            stdout.trim().to_string()
        },
        state: MergeState {
            in_progress: false,
            ours: String::new(),
            theirs: String::new(),
            files: Vec::new(),
        },
    })
}

pub fn merge_abort(repo: &Path) -> Result<String, String> {
    run_git(repo, &["merge", "--abort"])
}

pub fn merge_continue(repo: &Path) -> Result<String, String> {
    let state = merge_state(repo)?;
    if !state.files.is_empty() {
        return Err(format!("还有 {} 个冲突文件未处理", state.files.len()));
    }
    if !merge_in_progress(repo) {
        return Err("当前没有进行中的合并".to_string());
    }
    run_git(repo, &["commit", "--no-edit"])
}

pub fn merge_state(repo: &Path) -> Result<MergeState, String> {
    if !merge_in_progress(repo) {
        return Ok(MergeState {
            in_progress: false,
            ours: current_branch(repo),
            theirs: String::new(),
            files: Vec::new(),
        });
    }
    Ok(MergeState {
        in_progress: true,
        ours: current_branch(repo),
        theirs: incoming_branch(repo),
        files: conflict_files(repo)?,
    })
}

pub fn resolve_conflicts(repo: &Path, paths: &[String], side: &str) -> Result<MergeState, String> {
    if side != "ours" && side != "theirs" {
        return Err("未知的解决方式".to_string());
    }
    let files = conflict_files(repo)?;
    for path in paths {
        let actual = display_to_path(path);
        let Some(file) = files.iter().find(|item| item.path == actual) else {
            continue;
        };
        let deleted = if side == "ours" {
            file.ours_status == "已删除"
        } else {
            file.theirs_status == "已删除"
        };
        let result = if deleted {
            run_git(repo, &["rm", "-f", "--", &actual]).map(|_| ())
        } else {
            run_git(repo, &["checkout", &format!("--{side}"), "--", &actual])
                .and_then(|_| run_git(repo, &["add", "--", &actual]))
                .map(|_| ())
        };
        result.map_err(|err| format!("{actual}：{err}"))?;
    }
    merge_state(repo)
}

pub fn write_resolved(repo: &Path, path: &str, content: &str) -> Result<MergeState, String> {
    let actual = display_to_path(path);
    let reference = show_blob(repo, ":2", &actual)
        .or_else(|| show_blob(repo, "HEAD", &actual))
        .or_else(|| show_blob(repo, ":3", &actual))
        .or_else(|| std::fs::read(repo.join(&actual)).ok());
    let format = reference
        .map(|bytes| decode_text(&bytes).1)
        .unwrap_or_default();
    let full = repo.join(&actual);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("无法创建目录：{e}"))?;
    }
    std::fs::write(&full, encode_text(content, format)).map_err(|e| format!("写入解决结果失败：{e}"))?;
    run_git(repo, &["add", "--", &actual])?;
    merge_state(repo)
}

/// 用比较面板里拼好的内容覆盖工作树文件，沿用原文件的编码、BOM、换行符和末尾换行。
pub fn write_worktree_file(repo: &Path, path: &str, content: &str) -> Result<(), String> {
    let relative = Path::new(path);
    if path.trim().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_) | std::path::Component::CurDir))
    {
        return Err(format!("非法的文件路径：{path}"));
    }
    let full = repo.join(relative);
    let (format, trailing_newline) = match std::fs::read(&full) {
        Ok(bytes) => (decode_text(&bytes).1, bytes.is_empty() || bytes.ends_with(b"\n")),
        Err(_) => (TextFormat::default(), true),
    };
    let mut text = content.replace("\r\n", "\n");
    while text.ends_with('\n') {
        text.pop();
    }
    if trailing_newline && !text.is_empty() {
        text.push('\n');
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("无法创建目录：{e}"))?;
    }
    std::fs::write(&full, encode_text(&text, format)).map_err(|e| format!("写入文件失败：{e}"))
}

fn merge_in_progress(repo: &Path) -> bool {
    exec_git(repo, &["rev-parse", "-q", "--verify", "MERGE_HEAD"])
        .map(|(ok, _, _)| ok)
        .unwrap_or(false)
}

fn current_branch(repo: &Path) -> String {
    run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "HEAD".to_string())
}

fn incoming_branch(repo: &Path) -> String {
    if let Ok(msg_path) = run_git(repo, &["rev-parse", "--git-path", "MERGE_MSG"]) {
        let path = PathBuf::from(msg_path.trim());
        let full = if path.is_absolute() {
            path
        } else {
            repo.join(path)
        };
        if let Ok(msg) = std::fs::read_to_string(full) {
            if let Some(name) = msg.lines().next().and_then(|line| line.split('\'').nth(1)) {
                return name.to_string();
            }
        }
    }
    match run_git(
        repo,
        &[
            "name-rev",
            "--name-only",
            "--refs=refs/heads/*",
            "--refs=refs/remotes/*",
            "MERGE_HEAD",
        ],
    ) {
        Ok(name) => {
            let name = name.trim();
            if name.is_empty() || name == "undefined" {
                "MERGE_HEAD".to_string()
            } else {
                name.to_string()
            }
        }
        Err(_) => "MERGE_HEAD".to_string(),
    }
}

fn conflict_files(repo: &Path) -> Result<Vec<ConflictFile>, String> {
    let raw = run_git(repo, &["status", "--porcelain=v1"])?;
    let mut files = Vec::new();
    for line in raw.lines() {
        if line.len() < 4 {
            continue;
        }
        let x = line.as_bytes()[0] as char;
        let y = line.as_bytes()[1] as char;
        if !matches!((x, y), ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D')) {
            continue;
        }
        let (ours_status, theirs_status) = unmerged_labels(x, y);
        files.push(ConflictFile {
            path: porcelain_path(line),
            ours_status,
            theirs_status,
        });
    }
    Ok(files)
}

fn porcelain_path(line: &str) -> String {
    let rest = line.get(3..).unwrap_or("").trim();
    if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
        rest[1..rest.len() - 1]
            .replace("\\\\", "\\")
            .replace("\\\"", "\"")
            .replace("\\t", "\t")
    } else {
        rest.to_string()
    }
}

fn unmerged_labels(ours: char, theirs: char) -> (String, String) {
    let label = |code: char| -> String {
        match code {
            'A' => "已添加",
            'D' => "已删除",
            _ => "已修改",
        }
        .to_string()
    };
    match (ours, theirs) {
        ('A', 'U') => ("已添加".to_string(), "已删除".to_string()),
        ('U', 'A') => ("已删除".to_string(), "已添加".to_string()),
        ('D', 'U') => ("已删除".to_string(), "已修改".to_string()),
        ('U', 'D') => ("已修改".to_string(), "已删除".to_string()),
        _ => (label(ours), label(theirs)),
    }
}


fn name_status_map(repo: &Path, from: &str, to: &str) -> Result<std::collections::HashMap<String, String>, String> {
    let raw = run_git(repo, &["diff", "--name-status", "--no-renames", from, to])?;
    let mut map = std::collections::HashMap::new();
    for line in raw.lines() {
        let mut parts = line.splitn(2, '\t');
        let code = parts.next().unwrap_or("M").chars().next().unwrap_or('M');
        let path = parts.next().unwrap_or("").trim();
        if path.is_empty() {
            continue;
        }
        map.insert(path.to_string(), status_letter(code));
    }
    Ok(map)
}

fn exists_at(repo: &Path, rev: &str, path: &str) -> bool {
    run_git(repo, &["cat-file", "-e", &format!("{rev}:{path}")]).is_ok()
}

const MAX_MERGE_BYTES: usize = 512 * 1024;

fn show_blob(repo: &Path, rev: &str, path: &str) -> Option<Vec<u8>> {
    match exec_git_raw(repo, &["show", &format!("{rev}:{path}")]) {
        Ok((true, bytes, _)) => Some(bytes),
        _ => None,
    }
}

#[derive(Clone, Copy, Default)]
struct TextFormat {
    bom: bool,
    gbk: bool,
    crlf: bool,
}

fn decode_text(bytes: &[u8]) -> (String, TextFormat) {
    let bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let body = if bom { &bytes[3..] } else { bytes };
    let (text, gbk) = match std::str::from_utf8(body) {
        Ok(text) => (text.to_string(), false),
        Err(_) => {
            let (text, had_errors) = encoding_rs::GBK.decode_without_bom_handling(body);
            if had_errors {
                (String::from_utf8_lossy(body).into_owned(), false)
            } else {
                (text.into_owned(), true)
            }
        }
    };
    let crlf = text.contains("\r\n");
    (text, TextFormat { bom, gbk, crlf })
}

fn encode_text(content: &str, format: TextFormat) -> Vec<u8> {
    let normalized = content.replace("\r\n", "\n");
    let text = if format.crlf {
        normalized.replace('\n', "\r\n")
    } else {
        normalized
    };
    let mut out = Vec::with_capacity(text.len() + 3);
    if format.bom {
        out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    if format.gbk {
        out.extend_from_slice(&encoding_rs::GBK.encode(&text).0);
    } else {
        out.extend_from_slice(text.as_bytes());
    }
    out
}

fn localize_git_error(err: &str) -> String {
    let lower = err.to_lowercase();
    if lower.contains("please tell me who you are")
        || (lower.contains("user.email") && lower.contains("identify"))
    {
        return "请先配置 Git 用户信息：git config --global user.name 与 user.email".to_string();
    }
    if lower.contains("nothing to commit") {
        return "没有可提交的变更".to_string();
    }
    if lower.contains("no upstream") || lower.contains("no tracking information") || lower.contains("has no upstream") {
        return "当前分支没有设置上游，无法拉取或推送".to_string();
    }
    if lower.contains("authentication failed") || lower.contains("could not read from remote") {
        return "远程认证失败，或无法连接远程仓库".to_string();
    }
    if lower.contains("not a git repository") {
        return "所选目录不是 Git 仓库（需要包含 .git）".to_string();
    }
    if lower.contains("not possible to fast-forward") || lower.contains("divergent branches") {
        return "无法快进拉取，本地与远程存在分叉".to_string();
    }
    if lower.contains("conflict") || lower.contains("needs merge") {
        return "存在冲突，请先处理后再继续".to_string();
    }
    if lower.contains("pathspec") && lower.contains("did not match") {
        return "找不到指定的文件或分支".to_string();
    }
    if lower.contains("already exists") && !lower.contains("index.lock") {
        return "目标分支或引用已存在".to_string();
    }
    if lower.contains("index.lock") {
        return "Git 索引被占用：可能有未结束的 Git 进程，或上次操作留下了 index.lock。请稍后再试".to_string();
    }
    err.to_string()
}

fn is_empty_history(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("does not have any commits")
        || lower.contains("bad default revision")
        || lower.contains("unknown revision")
        || lower.contains("needed a single revision")
}

fn display_to_path(path: &str) -> String {
    if let Some((_, to)) = path.split_once(" → ") {
        to.to_string()
    } else {
        path.to_string()
    }
}

fn format_unix(timestamp: i64) -> String {
    if timestamp <= 0 {
        return String::new();
    }
    timestamp.to_string()
}

fn run_git_env(repo: &Path, args: &[&str], envs: &[(&str, String)]) -> Result<String, String> {
    with_index_lock_retry(repo, || exec_git_env(repo, args, envs))
}

fn exec_git_env(repo: &Path, args: &[&str], envs: &[(&str, String)]) -> Result<(bool, String, String), String> {
    let mut cmd = Command::new("git");
    cmd.arg("-c")
        .arg("core.quotepath=false")
        .arg("-c")
        .arg("i18n.logOutputEncoding=utf-8")
        .arg("-c")
        .arg(format!("core.editor={}", non_interactive_editor()))
        .arg("-c")
        .arg(format!("sequence.editor={}", non_interactive_editor()))
        .args(args)
        .current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_EDITOR", non_interactive_editor())
        .env("GIT_SEQUENCE_EDITOR", non_interactive_editor())
        .env("GIT_MERGE_AUTOEDIT", "no")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        cmd.env(key, value);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("无法执行 git，请确认已安装并加入 PATH：{e}"))?;
    Ok((output.status.success(), decode(&output.stdout), decode(&output.stderr)))
}

pub fn cherry_pick(repo: &Path, hash: &str) -> Result<String, String> {
    run_git(repo, &["cherry-pick", "--allow-empty", hash])
}

pub fn checkout_revision(repo: &Path, hash: &str) -> Result<String, String> {
    run_git(repo, &["checkout", "--detach", hash])?;
    Ok("已检出该修订（游离 HEAD）".to_string())
}

pub fn reset_to(repo: &Path, hash: &str, mode: &str) -> Result<String, String> {
    let flag = match mode {
        "soft" => "--soft",
        "hard" => "--hard",
        _ => "--mixed",
    };
    run_git(repo, &["reset", flag, hash])?;
    Ok("已将当前分支重置到该提交".to_string())
}

pub fn revert_commit(repo: &Path, hash: &str) -> Result<String, String> {
    run_git(repo, &["revert", "--no-edit", hash])
}

pub fn undo_commit(repo: &Path) -> Result<String, String> {
    run_git(repo, &["reset", "--soft", "HEAD~1"])?;
    Ok("已撤销最近一次提交，变更仍保留在暂存区".to_string())
}

pub fn rename_branch(repo: &Path, old: &str, new: &str) -> Result<String, String> {
    if new.trim().is_empty() {
        return Err("请输入新的分支名".to_string());
    }
    run_git(repo, &["branch", "-m", old, new.trim()])?;
    Ok(format!("已将分支 {old} 重命名为 {}", new.trim()))
}

pub fn set_upstream(repo: &Path, local: &str, upstream: &str) -> Result<String, String> {
    if upstream.trim().is_empty() {
        return Err("没有可跟踪的远程分支".to_string());
    }
    run_git(repo, &["branch", "--set-upstream-to", upstream.trim(), local])?;
    Ok(format!("已将 {local} 跟踪到 {}", upstream.trim()))
}

pub fn update_branch(repo: &Path, name: &str) -> Result<String, String> {
    fetch(repo)?;
    let current = current_branch(repo);
    let upstream = branch_upstream(repo, name);
    if current == name {
        if upstream.is_empty() {
            return Err("当前分支没有跟踪远程，无法更新".to_string());
        }
        return run_git(repo, &["pull", "--ff-only"]);
    }
    if name.contains('/') {
        return Ok("已获取远程更新".to_string());
    }
    let source = if !upstream.is_empty() {
        upstream
    } else {
        format!("origin/{name}")
    };
    run_git(repo, &["fetch", ".", &format!("{source}:{name}")])
        .or_else(|_| run_git(repo, &["branch", "-f", name, &source]))?;
    Ok(format!("已更新分支 {name}"))
}

const MAX_PATCH_LINES: usize = 20_000;

/// `to` 为空表示与工作树（含未提交和未跟踪的改动）比较。
pub fn rev_diff_files(repo: &Path, from: &str, to: &str) -> Result<Vec<GitFile>, String> {
    if from.trim().is_empty() {
        return Err("缺少比较的起点".to_string());
    }
    let mut args = vec!["diff", "--name-status", "-M", "-z", from];
    if !to.is_empty() {
        args.push(to);
    }
    let raw = run_git(repo, &args)?;
    let mut tokens = raw.split('\0').filter(|item| !item.is_empty());
    let mut files = Vec::new();
    while let Some(code) = tokens.next() {
        let letter = code.chars().next().unwrap_or('M');
        let Some(first) = tokens.next() else { break };
        let path = if matches!(letter, 'R' | 'C') {
            match tokens.next() {
                Some(second) => format!("{first} → {second}"),
                None => first.to_string(),
            }
        } else {
            first.to_string()
        };
        files.push(GitFile {
            path,
            status: status_letter(letter),
            staged: false,
            untracked: false,
        });
    }
    if to.is_empty() {
        let others = run_git(repo, &["ls-files", "--others", "--exclude-standard", "-z"])?;
        for path in others.split('\0').filter(|item| !item.is_empty()) {
            if let Some(existing) = files.iter_mut().find(|file| file.path == path) {
                existing.status = "M".to_string();
                existing.untracked = true;
                continue;
            }
            files.push(GitFile {
                path: path.to_string(),
                status: "A".to_string(),
                staged: false,
                untracked: true,
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

pub fn rev_diff_file(
    repo: &Path,
    from: &str,
    to: &str,
    path: &str,
    untracked: bool,
    context: u32,
    ignore_whitespace: bool,
) -> Result<FileDiff, String> {
    let (old_path, new_path) = match path.split_once(" → ") {
        Some((old, new)) => (old.to_string(), new.to_string()),
        None => (path.to_string(), path.to_string()),
    };
    let unified = format!("-U{}", context.min(1_000_000));
    let bytes = if untracked {
        match show_blob(repo, from, &new_path) {
            Some(base) => untracked_against_blob(repo, &new_path, &base, &unified, ignore_whitespace)?,
            None => return untracked_diff(repo, &new_path).map(|diff| truncate_patch(diff)),
        }
    } else {
        let mut args = vec!["diff", unified.as_str(), "-M"];
        if ignore_whitespace {
            args.push("-w");
        }
        args.push(from);
        if !to.is_empty() {
            args.push(to);
        }
        args.push("--");
        args.push(&old_path);
        if new_path != old_path {
            args.push(&new_path);
        }
        let (ok, stdout, stderr) = exec_git_raw(repo, &args)?;
        if !ok {
            return Err(localize_git_error(decode(&stderr).trim()));
        }
        stdout
    };
    let patch = decode_text(&bytes).0;
    let binary = patch.lines().any(|line| line.starts_with("Binary files ") || line.starts_with("GIT binary patch"));
    Ok(truncate_patch(FileDiff {
        path: new_path,
        binary,
        patch: if binary { String::new() } else { patch },
    }))
}

fn untracked_against_blob(
    repo: &Path,
    path: &str,
    base: &[u8],
    unified: &str,
    ignore_whitespace: bool,
) -> Result<Vec<u8>, String> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let tmp = std::env::temp_dir().join(format!("rebased-base-{}-{unique}", std::process::id()));
    std::fs::write(&tmp, base).map_err(|e| format!("无法写入临时文件：{e}"))?;
    let tmp_arg = tmp.to_string_lossy().to_string();
    let mut args = vec!["diff", "--no-index", unified];
    if ignore_whitespace {
        args.push("-w");
    }
    args.extend(["--", tmp_arg.as_str(), path]);
    let result = exec_git_raw(repo, &args);
    let _ = std::fs::remove_file(&tmp);
    let (_, stdout, stderr) = result?;
    if stdout.is_empty() && !stderr.is_empty() {
        return Err(localize_git_error(decode(&stderr).trim()));
    }
    Ok(stdout)
}

fn truncate_patch(mut diff: FileDiff) -> FileDiff {
    let total = diff.patch.lines().count();
    if total > MAX_PATCH_LINES {
        let mut kept: String = diff
            .patch
            .lines()
            .take(MAX_PATCH_LINES)
            .collect::<Vec<_>>()
            .join("\n");
        kept.push_str(&format!("\n@@ -0 +0 @@ 差异过大，仅显示前 {MAX_PATCH_LINES} 行（共 {total} 行）"));
        diff.patch = kept;
    }
    diff
}

fn branch_upstream(repo: &Path, name: &str) -> String {
    run_git(repo, &["rev-parse", "--abbrev-ref", &format!("{name}@{{upstream}}")])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn create_branch_at(repo: &Path, name: &str, hash: &str, checkout: bool) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("请输入分支名".to_string());
    }
    run_git(repo, &["branch", name.trim(), hash])?;
    if checkout {
        run_git(repo, &["checkout", name.trim()])?;
    }
    Ok(format!("已创建分支 {name}"))
}

pub fn delete_branch(repo: &Path, name: &str, force: bool) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("请输入分支名".to_string());
    }
    let flag = if force { "-D" } else { "-d" };
    run_git(repo, &["branch", flag, name.trim()])?;
    Ok(format!("已删除分支 {name}"))
}

pub fn delete_tag(repo: &Path, name: &str) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("请输入标签名".to_string());
    }
    run_git(repo, &["tag", "-d", name.trim()])?;
    Ok(format!("已删除标签 {name}"))
}

pub fn push_tag(repo: &Path, remote: &str, name: &str) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("请输入标签名".to_string());
    }
    let remote = if remote.trim().is_empty() { "origin" } else { remote.trim() };
    run_git(repo, &["push", remote, name.trim()])?;
    Ok(format!("已推送标签 {name}"))
}

pub fn add_remote(repo: &Path, name: &str, url: &str) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("请输入远程名称".to_string());
    }
    if url.trim().is_empty() {
        return Err("请输入远程 URL".to_string());
    }
    run_git(repo, &["remote", "add", name.trim(), url.trim()])?;
    Ok(format!("已添加远程 {}", name.trim()))
}

pub fn remove_remote(repo: &Path, name: &str) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("请输入远程名称".to_string());
    }
    run_git(repo, &["remote", "remove", name.trim()])?;
    Ok(format!("已移除远程 {}", name.trim()))
}

pub fn set_remote_url(repo: &Path, name: &str, url: &str) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("请输入远程名称".to_string());
    }
    if url.trim().is_empty() {
        return Err("请输入远程 URL".to_string());
    }
    run_git(repo, &["remote", "set-url", name.trim(), url.trim()])?;
    Ok(format!("已更新远程 {} 的 URL", name.trim()))
}

pub fn apply_patch(repo: &Path, path: &str) -> Result<String, String> {
    if path.trim().is_empty() {
        return Err("请选择补丁文件".to_string());
    }
    run_git(repo, &["apply", "--ignore-whitespace", path])?;
    Ok(format!("已应用补丁 {}", Path::new(path).file_name().unwrap_or_default().to_string_lossy()))
}

pub fn add_worktree(repo: &Path, path: &str, branch: &str, create_branch: bool) -> Result<String, String> {
    if path.trim().is_empty() {
        return Err("请输入工作树路径".to_string());
    }
    if branch.trim().is_empty() {
        return Err("请输入分支名".to_string());
    }
    let mut args = vec!["worktree", "add"];
    if create_branch {
        args.push("-b");
    }
    args.push(path.trim());
    args.push(branch.trim());
    run_git(repo, &args)?;
    Ok(format!("已添加工作树 {}", path.trim()))
}

pub fn remove_worktree(repo: &Path, path: &str) -> Result<String, String> {
    if path.trim().is_empty() {
        return Err("请输入工作树路径".to_string());
    }
    run_git(repo, &["worktree", "remove", path.trim(), "--force"])?;
    Ok(format!("已移除工作树 {}", path.trim()))
}

pub fn create_tag_at(repo: &Path, name: &str, hash: &str) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("请输入标签名".to_string());
    }
    run_git(repo, &["tag", name.trim(), hash])?;
    Ok(format!("已创建标签 {name}"))
}

pub fn format_patch(repo: &Path, hash: &str) -> Result<String, String> {
    run_git(repo, &["format-patch", "-1", "--stdout", hash])
}

pub fn write_text_file(path: &str, content: &str) -> Result<(), String> {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("无法创建目录：{e}"))?;
    }
    std::fs::write(path, content).map_err(|e| format!("写入文件失败：{e}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub path: String,
    pub content: String,
    pub binary: bool,
    pub truncated: bool,
    pub size: u64,
}

const PREVIEW_LIMIT: usize = 512 * 1024;

fn normalize_rel(rel: &str) -> Result<String, String> {
    let rel = rel.replace('\\', "/").trim_matches('/').to_string();
    if rel.is_empty() || rel.contains("..") || Path::new(&rel).is_absolute() {
        return Err("无效的文件路径".to_string());
    }
    if rel
        .split('/')
        .any(|part| part.is_empty() || part.eq_ignore_ascii_case(".git"))
    {
        return Err("无效的文件路径".to_string());
    }
    Ok(rel)
}

fn repo_root(repo: &Path) -> Result<PathBuf, String> {
    repo.canonicalize()
        .map_err(|e| format!("无法解析仓库路径：{e}"))
}

fn ensure_inside(root: &Path, path: &Path) -> Result<(), String> {
    let mut cur = path;
    loop {
        if cur.exists() {
            let canon = cur
                .canonicalize()
                .map_err(|e| format!("无法解析路径：{e}"))?;
            if !canon.starts_with(root) {
                return Err("文件不在仓库内".to_string());
            }
            return Ok(());
        }
        match cur.parent() {
            Some(parent) if parent != cur => cur = parent,
            _ => return Err("文件不在仓库内".to_string()),
        }
    }
}

fn resolve_worktree_file(repo: &Path, rel: &str) -> Result<(String, PathBuf), String> {
    let rel = normalize_rel(rel)?;
    let repo_root = repo_root(repo)?;
    let full = repo_root.join(&rel);
    let canonical = full.canonicalize().map_err(|_| "文件不存在".to_string())?;
    if !canonical.starts_with(&repo_root) {
        return Err("文件不在仓库内".to_string());
    }
    Ok((rel, canonical))
}

fn resolve_worktree_dest(repo: &Path, rel: &str) -> Result<(String, PathBuf), String> {
    let rel = normalize_rel(rel)?;
    let repo_root = repo_root(repo)?;
    let full = repo_root.join(&rel);
    ensure_inside(&repo_root, &full)?;
    Ok((rel, full))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("无法创建目录：{e}"))?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("无法读取目录：{e}"))? {
        let entry = entry.map_err(|e| format!("无法读取目录：{e}"))?;
        if entry.file_name().eq_ignore_ascii_case(".git") {
            continue;
        }
        let src = entry.path();
        let dest = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir(&src, &dest)?;
        } else {
            std::fs::copy(&src, &dest).map_err(|e| format!("复制失败：{e}"))?;
        }
    }
    Ok(())
}

pub fn copy_worktree(repo: &Path, src: &str, dest: &str) -> Result<String, String> {
    let (_, src_path) = resolve_worktree_file(repo, src)?;
    let (dest_rel, dest_path) = resolve_worktree_dest(repo, dest)?;
    if dest_path.exists() {
        return Err("目标已存在".to_string());
    }
    if src_path.is_dir() {
        let src_canon = src_path
            .canonicalize()
            .map_err(|e| format!("无法解析路径：{e}"))?;
        if dest_path.starts_with(&src_canon) {
            return Err("不能复制到自身目录内".to_string());
        }
        copy_dir(&src_path, &dest_path)?;
    } else {
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("无法创建目录：{e}"))?;
        }
        std::fs::copy(&src_path, &dest_path).map_err(|e| format!("复制失败：{e}"))?;
    }
    Ok(dest_rel)
}

pub fn rename_worktree(repo: &Path, src: &str, dest: &str) -> Result<String, String> {
    let (_, src_path) = resolve_worktree_file(repo, src)?;
    let (dest_rel, dest_path) = resolve_worktree_dest(repo, dest)?;
    if dest_path.exists() {
        return Err("目标已存在".to_string());
    }
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("无法创建目录：{e}"))?;
    }
    std::fs::rename(&src_path, &dest_path).map_err(|e| format!("重命名失败：{e}"))?;
    Ok(dest_rel)
}

pub fn delete_worktree(repo: &Path, rel: &str) -> Result<(), String> {
    let (_, path) = resolve_worktree_file(repo, rel)?;
    let root = repo_root(repo)?;
    if path == root {
        return Err("不能删除仓库根目录".to_string());
    }
    if path.is_dir() {
        std::fs::remove_dir_all(&path).map_err(|e| format!("删除失败：{e}"))?;
    } else {
        std::fs::remove_file(&path).map_err(|e| format!("删除失败：{e}"))?;
    }
    Ok(())
}

pub fn list_worktree(repo: &Path) -> Result<Vec<String>, String> {
    let root = repo
        .canonicalize()
        .map_err(|e| format!("无法解析仓库路径：{e}"))?;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![root.clone()];
    seen.insert(root.clone());
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.eq_ignore_ascii_case(".git") {
                continue;
            }
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(&root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if rel.is_empty() {
                continue;
            }
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if ft.is_dir() && !ft.is_symlink() {
                if let Ok(canon) = path.canonicalize() {
                    if !seen.insert(canon) {
                        continue;
                    }
                }
                out.push(format!("{rel}/"));
                stack.push(path);
            } else {
                out.push(rel);
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn set_clipboard_files(paths: &[String]) -> Result<(), String> {
    #[cfg(windows)]
    {
        return windows_clipboard::set_files(paths);
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
        Err("当前系统不支持复制文件资源".to_string())
    }
}

pub fn copy_worktree_to_clipboard(repo: &Path, rel: &str) -> Result<(), String> {
    let (_, path) = resolve_worktree_file(repo, rel)?;
    set_clipboard_files(&[os_display_path(&path)])
}

#[cfg(windows)]
mod windows_clipboard {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    const CF_HDROP: u32 = 15;
    const GMEM_MOVEABLE: u32 = 0x0002;
    const GMEM_ZEROINIT: u32 = 0x0040;
    const DROPEFFECT_COPY: u32 = 1;

    #[repr(C)]
    struct DropFiles {
        p_files: u32,
        pt_x: i32,
        pt_y: i32,
        f_nc: i32,
        f_wide: i32,
    }

    extern "system" {
        fn OpenClipboard(hwnd: *mut core::ffi::c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(format: u32, mem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn GlobalAlloc(flags: u32, bytes: usize) -> *mut core::ffi::c_void;
        fn GlobalLock(mem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn GlobalUnlock(mem: *mut core::ffi::c_void) -> i32;
        fn RegisterClipboardFormatW(name: *const u16) -> u32;
    }

    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text).encode_wide().chain(std::iter::once(0)).collect()
    }

    fn alloc_bytes(bytes: &[u8]) -> Result<*mut core::ffi::c_void, String> {
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len());
            if handle.is_null() {
                return Err("无法分配剪贴板内存".to_string());
            }
            let locked = GlobalLock(handle);
            if locked.is_null() {
                return Err("无法写入剪贴板内存".to_string());
            }
            ptr::copy_nonoverlapping(bytes.as_ptr(), locked as *mut u8, bytes.len());
            GlobalUnlock(handle);
            Ok(handle)
        }
    }

    pub fn set_files(paths: &[String]) -> Result<(), String> {
        if paths.is_empty() {
            return Err("没有可复制的文件".to_string());
        }
        let mut list = Vec::<u16>::new();
        for path in paths {
            list.extend(wide(path));
        }
        list.push(0);
        let header = DropFiles {
            p_files: std::mem::size_of::<DropFiles>() as u32,
            pt_x: 0,
            pt_y: 0,
            f_nc: 0,
            f_wide: 1,
        };
        let mut blob = vec![0u8; std::mem::size_of::<DropFiles>() + list.len() * 2];
        unsafe {
            ptr::copy_nonoverlapping(
                &header as *const DropFiles as *const u8,
                blob.as_mut_ptr(),
                std::mem::size_of::<DropFiles>(),
            );
            ptr::copy_nonoverlapping(
                list.as_ptr() as *const u8,
                blob.as_mut_ptr().add(std::mem::size_of::<DropFiles>()),
                list.len() * 2,
            );
        }
        let drop_handle = alloc_bytes(&blob)?;
        let effect = DROPEFFECT_COPY.to_le_bytes();
        let effect_handle = alloc_bytes(&effect)?;
        let format_name = wide("Preferred DropEffect");
        unsafe {
            let mut opened = false;
            for _ in 0..20 {
                if OpenClipboard(ptr::null_mut()) != 0 {
                    opened = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            if !opened {
                return Err("无法打开剪贴板".to_string());
            }
            EmptyClipboard();
            if SetClipboardData(CF_HDROP, drop_handle).is_null() {
                CloseClipboard();
                return Err("写入文件到剪贴板失败".to_string());
            }
            let effect_format = RegisterClipboardFormatW(format_name.as_ptr());
            if effect_format != 0 {
                let _ = SetClipboardData(effect_format, effect_handle);
            }
            CloseClipboard();
        }
        Ok(())
    }
}

fn os_display_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    raw.strip_prefix(r"\\?\")
        .unwrap_or(&raw)
        .replace('/', "\\")
}

pub fn abs_worktree_path(repo: &Path, rel: &str) -> Result<String, String> {
    Ok(os_display_path(&resolve_worktree_file(repo, rel)?.1))
}

pub fn reveal_in_os(repo: &Path, rel: &str) -> Result<(), String> {
    let (_, canonical) = resolve_worktree_file(repo, rel)?;
    let path = os_display_path(&canonical);
    let mut cmd = std::process::Command::new("explorer");
    if canonical.is_dir() {
        cmd.arg(&path);
    } else {
        cmd.arg(format!("/select,{path}"));
    }
    cmd.spawn().map_err(|e| format!("无法打开资源管理器：{e}"))?;
    Ok(())
}

pub fn open_in_os(repo: &Path, rel: &str) -> Result<(), String> {
    let path = os_display_path(&resolve_worktree_file(repo, rel)?.1);
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", "start", "", &path]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn()
        .map_err(|e| format!("无法用关联程序打开：{e}"))?;
    Ok(())
}

pub fn save_worktree_file(repo: &Path, rel: &str, content: &str) -> Result<FilePreview, String> {
    let (rel, canonical) = resolve_worktree_file(repo, rel)?;
    let meta = std::fs::metadata(&canonical).map_err(|e| format!("无法读取文件：{e}"))?;
    if meta.is_dir() {
        return Err("这是一个目录".to_string());
    }
    std::fs::write(&canonical, content).map_err(|e| format!("写入文件失败：{e}"))?;
    preview_file(repo, &rel)
}

pub fn preview_file(repo: &Path, rel: &str) -> Result<FilePreview, String> {
    let (rel, canonical) = resolve_worktree_file(repo, rel)?;
    let meta = std::fs::metadata(&canonical).map_err(|e| format!("无法读取文件：{e}"))?;
    if meta.is_dir() {
        return Err("这是一个目录".to_string());
    }
    let size = meta.len();
    let bytes = std::fs::read(&canonical).map_err(|e| format!("读取失败：{e}"))?;
    if bytes.contains(&0) {
        return Ok(FilePreview {
            path: rel,
            content: String::new(),
            binary: true,
            truncated: false,
            size,
        });
    }
    let truncated = bytes.len() > PREVIEW_LIMIT;
    let slice = if truncated {
        &bytes[..PREVIEW_LIMIT]
    } else {
        &bytes
    };
    Ok(FilePreview {
        path: rel,
        content: String::from_utf8_lossy(slice).into_owned(),
        binary: false,
        truncated,
        size,
    })
}

pub fn drop_commit(repo: &Path, hash: &str) -> Result<String, String> {
    let parent = run_git(repo, &["rev-parse", &format!("{hash}^")])
        .map_err(|_| "无法删除根提交".to_string())?;
    run_git(repo, &["rebase", "--onto", parent.trim(), hash])?;
    Ok("已删除该提交".to_string())
}

pub fn push_upto(repo: &Path, remote: &str, branch: &str, hash: &str) -> Result<String, String> {
    let remote = if remote.trim().is_empty() { "origin" } else { remote.trim() };
    if branch.trim().is_empty() {
        return Err("当前没有可推送的分支".to_string());
    }
    run_git(
        repo,
        &["push", remote, &format!("{hash}:refs/heads/{branch}")],
    )
}

pub fn squash_commit(repo: &Path, hash: &str) -> Result<String, String> {
    let parent = run_git(repo, &["rev-parse", &format!("{hash}^")])
        .map_err(|_| "无法压缩根提交".to_string())?;
    let raw = run_git(
        repo,
        &[
            "log",
            "--reverse",
            "--pretty=%H",
            &format!("{}..HEAD", parent.trim()),
        ],
    )?;
    let items: Vec<RebaseItem> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| RebaseItem {
            action: if line.trim() == hash { "squash" } else { "pick" }.to_string(),
            hash: line.trim().to_string(),
        })
        .collect();
    if items.is_empty() {
        return Err("没有可压缩的提交".to_string());
    }
    rebase_plan(repo, parent.trim(), &items)
}

pub fn fixup_commit(repo: &Path, hash: &str) -> Result<String, String> {
    run_git(repo, &["commit", "--fixup", hash])?;
    if let Ok(parent) = run_git(repo, &["rev-parse", &format!("{hash}^")]) {
        let _ = run_git_env(
            repo,
            &["rebase", "-i", "--autosquash", parent.trim()],
            &[
                ("GIT_SEQUENCE_EDITOR", true_editor()),
                ("GIT_EDITOR", true_editor()),
            ],
        );
    }
    Ok("已创建 fixup 提交并尝试自动压入".to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseItem {
    pub action: String,
    pub hash: String,
}

pub fn rebase_plan(repo: &Path, upstream: &str, items: &[RebaseItem]) -> Result<String, String> {
    let mut lines = String::new();
    for item in items {
        let action = match item.action.as_str() {
            "squash" | "s" => "squash",
            "fixup" | "f" => "fixup",
            "drop" | "d" => "drop",
            _ => "pick",
        };
        lines.push_str(&format!("{action} {}\n", item.hash));
    }
    rebase_with_todo(repo, upstream, &lines)?;
    Ok("变基完成".to_string())
}

fn rebase_with_todo(repo: &Path, upstream: &str, todo: &str) -> Result<String, String> {
    if todo.trim().is_empty() {
        return Ok(String::new());
    }
    let tmp = std::env::temp_dir();
    let pid = std::process::id();
    let todo_path = tmp.join(format!("rebased-todo-{pid}.txt"));
    std::fs::write(&todo_path, todo).map_err(|e| format!("无法写入变基计划：{e}"))?;
    let editor = write_rebase_editor(&todo_path)?;
    run_git_env(
        repo,
        &["-c", "core.editor=true", "rebase", "-i", upstream],
        &[
            ("GIT_SEQUENCE_EDITOR", editor),
            ("GIT_EDITOR", true_editor()),
        ],
    )
}

fn write_rebase_editor(todo_path: &Path) -> Result<String, String> {
    let tmp = std::env::temp_dir();
    let pid = std::process::id();
    #[cfg(windows)]
    {
        let editor = tmp.join(format!("rebased-ed-{pid}.cmd"));
        let body = format!(
            "@echo off\r\ncopy /Y \"{}\" \"%~1\" >nul\r\n",
            todo_path.display()
        );
        std::fs::write(&editor, body).map_err(|e| format!("无法创建变基编辑器：{e}"))?;
        return Ok(editor.to_string_lossy().into_owned());
    }
    #[cfg(not(windows))]
    {
        let editor = tmp.join(format!("rebased-ed-{pid}.sh"));
        let body = format!("#!/bin/sh\ncp \"{}\" \"$1\"\n", todo_path.display());
        std::fs::write(&editor, body).map_err(|e| format!("无法创建变基编辑器：{e}"))?;
        let _ = std::process::Command::new("chmod").arg("+x").arg(&editor).status();
        Ok(editor.to_string_lossy().into_owned())
    }
}

fn non_interactive_editor() -> &'static str {
    if cfg!(windows) {
        "cmd /c exit 0"
    } else {
        "true"
    }
}

fn true_editor() -> String {
    non_interactive_editor().to_string()
}

#[cfg(test)]
mod checkout_tests {
    use super::checkout;
    use std::path::Path;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn git(repo: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("git should run");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[test]
    fn checkout_remote_branch_creates_tracking_local_branch() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rebased-checkout-{unique}"));
        let remote = root.join("remote.git");
        let repo = root.join("repo");
        std::fs::create_dir_all(&remote).expect("create remote dir");
        std::fs::create_dir_all(&repo).expect("create repo dir");

        git(&remote, &["init", "--bare"]);
        git(&repo, &["init"]);
        git(&repo, &["config", "user.name", "Rebased Test"]);
        git(&repo, &["config", "user.email", "rebased@example.test"]);
        std::fs::write(repo.join("README.md"), "test\n").expect("write fixture");
        git(&repo, &["add", "README.md"]);
        git(&repo, &["commit", "-m", "initial"]);
        git(&repo, &["remote", "add", "origin", remote.to_str().unwrap()]);
        git(&repo, &["checkout", "-b", "release-front_test"]);
        git(&repo, &["push", "-u", "origin", "release-front_test"]);
        git(&repo, &["checkout", "-"]);
        git(&repo, &["branch", "-D", "release-front_test"]);

        checkout(&repo, "origin/release-front_test").expect("remote checkout should succeed");

        assert_eq!(
            git(&repo, &["symbolic-ref", "--short", "HEAD"]),
            "release-front_test"
        );
        assert_eq!(
            git(&repo, &["rev-parse", "--abbrev-ref", "@{upstream}"]),
            "origin/release-front_test"
        );

        std::fs::remove_dir_all(root).expect("remove fixture");
    }
}

#[cfg(test)]
mod merge_tests {
    use super::{
        conflict_file, merge_branch, merge_state, resolve_conflicts, rev_diff_file, rev_diff_files, write_resolved,
        write_worktree_file,
    };
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn git(repo: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("git should run");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn fixture(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let repo = std::env::temp_dir().join(format!("rebased-merge-{name}-{unique}"));
        std::fs::create_dir_all(&repo).expect("create repo dir");
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.name", "Rebased Test"]);
        git(&repo, &["config", "user.email", "rebased@example.test"]);
        git(&repo, &["config", "core.autocrlf", "false"]);
        repo
    }

    fn commit_file(repo: &Path, path: &str, bytes: &[u8], message: &str) {
        let full = repo.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(full, bytes).expect("write fixture");
        git(repo, &["add", "--", path]);
        git(repo, &["commit", "-m", message]);
    }

    fn remote_pair(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let seed = fixture(&format!("{name}-seed"));
        commit_file(&seed, "shared.txt", b"base\n", "base");
        let root = seed.with_extension("pair");
        std::fs::create_dir_all(&root).expect("create pair root");
        git(&root, &["clone", "--bare", seed.to_str().unwrap(), "remote.git"]);
        std::fs::remove_dir_all(&seed).expect("remove seed");
        let clone = |dir: &str| {
            git(&root, &["clone", "remote.git", dir]);
            let path = root.join(dir);
            git(&path, &["config", "user.name", "Rebased Test"]);
            git(&path, &["config", "user.email", "rebased@example.test"]);
            git(&path, &["config", "core.autocrlf", "false"]);
            path
        };
        let mine = clone("mine");
        let other = clone("other");
        (root, mine, other)
    }

    #[test]
    fn sync_pull_and_push_auto_merge_diverged_branches() {
        let (root, mine, other) = remote_pair("sync");
        commit_file(&other, "remote.txt", b"remote\n", "remote side");
        git(&other, &["push"]);
        commit_file(&mine, "local.txt", b"local\n", "local side");

        let pulled = super::sync_pull(&mine).expect("pull");
        assert_eq!(pulled.status, "merged", "{}", pulled.message);
        assert!(mine.join("remote.txt").exists() && mine.join("local.txt").exists());

        commit_file(&other, "remote2.txt", b"r2\n", "remote again");
        git(&other, &["push"]);
        commit_file(&mine, "local2.txt", b"l2\n", "local again");
        let pushed = super::sync_push(&mine).expect("push");
        assert_eq!(pushed.status, "merged", "{}", pushed.message);
        git(&other, &["pull", "--ff-only"]);
        assert!(other.join("local2.txt").exists());

        commit_file(&other, "shared.txt", b"theirs\n", "remote edit");
        git(&other, &["push"]);
        commit_file(&mine, "shared.txt", b"ours\n", "local edit");
        let conflicted = super::sync_push(&mine).expect("push with conflict");
        assert_eq!(conflicted.status, "conflicts");
        assert_eq!(conflicted.conflicts, vec!["shared.txt".to_string()]);
        assert!(super::merge_in_progress(&mine));
        git(&mine, &["merge", "--abort"]);

        let conflicted = super::sync_pull(&mine).expect("pull with conflict");
        assert_eq!(conflicted.status, "conflicts");

        std::fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn merge_touching_conflict_named_file_is_not_a_conflict() {
        let repo = fixture("named");
        commit_file(&repo, "src/ConflictsDialog.vue", b"a\n", "initial");
        git(&repo, &["checkout", "-b", "feature"]);
        commit_file(&repo, "src/ConflictsDialog.vue", b"b\n", "change");
        git(&repo, &["checkout", "main"]);

        let outcome = merge_branch(&repo, "feature").expect("merge should succeed");
        assert_eq!(outcome.status, "ok");
        assert!(!outcome.state.in_progress);

        std::fs::remove_dir_all(repo).expect("remove fixture");
    }

    #[test]
    fn write_resolved_keeps_gbk_and_crlf() {
        let repo = fixture("format");
        let gbk = |text: &str| encoding_rs::GBK.encode(text).0.into_owned();
        commit_file(&repo, "a.txt", &gbk("标题\r\n原始\r\n"), "initial");
        git(&repo, &["checkout", "-b", "feature"]);
        commit_file(&repo, "a.txt", &gbk("标题\r\n传入\r\n"), "theirs");
        git(&repo, &["checkout", "main"]);
        commit_file(&repo, "a.txt", &gbk("标题\r\n本地\r\n"), "ours");

        let outcome = merge_branch(&repo, "feature").expect("merge should run");
        assert_eq!(outcome.status, "conflicts");

        let content = conflict_file(&repo, "a.txt").expect("conflict file");
        assert_eq!(content.ours, "标题\r\n本地\r\n");
        assert_eq!(content.theirs, "标题\r\n传入\r\n");

        let state = write_resolved(&repo, "a.txt", "标题\n本地\n传入\n").expect("write resolved");
        assert!(state.files.is_empty());
        let written = std::fs::read(repo.join("a.txt")).expect("read result");
        assert_eq!(written, gbk("标题\r\n本地\r\n传入\r\n"));

        std::fs::remove_dir_all(repo).expect("remove fixture");
    }

    #[test]
    fn large_conflict_file_is_flagged() {
        let repo = fixture("large");
        let big = |tail: &str| format!("{}{tail}\n", "x\n".repeat(300 * 1024));
        commit_file(&repo, "big.txt", big("base").as_bytes(), "initial");
        git(&repo, &["checkout", "-b", "feature"]);
        commit_file(&repo, "big.txt", big("theirs").as_bytes(), "theirs");
        git(&repo, &["checkout", "main"]);
        commit_file(&repo, "big.txt", big("ours").as_bytes(), "ours");
        merge_branch(&repo, "feature").expect("merge should run");

        let content = conflict_file(&repo, "big.txt").expect("conflict file");
        assert!(content.too_large);
        assert!(content.ours.is_empty());

        std::fs::remove_dir_all(repo).expect("remove fixture");
    }

    #[test]
    fn resolve_conflicts_handles_batch_with_deletion() {
        let repo = fixture("batch");
        commit_file(&repo, "keep.txt", b"base\n", "keep");
        commit_file(&repo, "gone.txt", b"base\n", "gone");
        git(&repo, &["checkout", "-b", "feature"]);
        commit_file(&repo, "keep.txt", b"theirs\n", "theirs keep");
        git(&repo, &["rm", "gone.txt"]);
        git(&repo, &["commit", "-m", "theirs delete"]);
        git(&repo, &["checkout", "main"]);
        commit_file(&repo, "keep.txt", b"ours\n", "ours keep");
        commit_file(&repo, "gone.txt", b"ours\n", "ours gone");
        merge_branch(&repo, "feature").expect("merge should run");
        assert_eq!(merge_state(&repo).expect("state").files.len(), 2);

        let paths = vec!["keep.txt".to_string(), "gone.txt".to_string()];
        let state = resolve_conflicts(&repo, &paths, "theirs").expect("resolve");
        assert!(state.files.is_empty());
        assert_eq!(std::fs::read(repo.join("keep.txt")).expect("read keep"), b"theirs\n");
        assert!(!repo.join("gone.txt").exists());

        std::fs::remove_dir_all(repo).expect("remove fixture");
    }

    #[test]
    fn rev_diff_lists_files_against_working_tree() {
        let repo = fixture("revdiff");
        let gbk = |text: &str| encoding_rs::GBK.encode(text).0.into_owned();
        commit_file(&repo, "keep.txt", b"same\n", "keep");
        commit_file(&repo, "edit.txt", b"one\n", "edit");
        commit_file(&repo, "old/name.txt", b"line1\nline2\nline3\nline4\n", "rename source");
        commit_file(&repo, "cn.txt", &gbk("中文\n"), "gbk");
        git(&repo, &["branch", "test"]);
        commit_file(&repo, "edit.txt", b"two\n", "edit on main");
        std::fs::create_dir_all(repo.join("new")).expect("create rename target");
        git(&repo, &["mv", "old/name.txt", "new/name.txt"]);
        git(&repo, &["commit", "-m", "rename"]);
        commit_file(&repo, "bin.dat", &[0, 1, 2, 3], "binary");
        std::fs::write(repo.join("cn.txt"), gbk("中文\n改动\n")).expect("dirty edit");
        std::fs::write(repo.join("fresh.txt"), b"brand new\n").expect("untracked");

        let files = rev_diff_files(&repo, "test", "").expect("list");
        let summary: Vec<(String, String, bool)> = files
            .iter()
            .map(|file| (file.path.clone(), file.status.clone(), file.untracked))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("bin.dat".to_string(), "A".to_string(), false),
                ("cn.txt".to_string(), "M".to_string(), false),
                ("edit.txt".to_string(), "M".to_string(), false),
                ("fresh.txt".to_string(), "A".to_string(), true),
                ("old/name.txt → new/name.txt".to_string(), "R".to_string(), false),
            ]
        );

        let cn = rev_diff_file(&repo, "test", "", "cn.txt", false, 3, false).expect("cn diff");
        assert!(cn.patch.contains("+改动"), "patch: {}", cn.patch);
        let fresh = rev_diff_file(&repo, "test", "", "fresh.txt", true, 3, false).expect("untracked diff");
        assert!(fresh.patch.contains("+brand new"));
        let renamed =
            rev_diff_file(&repo, "test", "", "old/name.txt → new/name.txt", false, 3, false).expect("rename diff");
        assert!(!renamed.binary);
        let full = rev_diff_file(&repo, "test", "", "edit.txt", false, 1_000_000, true).expect("full context");
        assert!(full.patch.contains("-one") && full.patch.contains("+two"));
        let bin = rev_diff_file(&repo, "test", "", "bin.dat", false, 3, false).expect("binary diff");
        assert!(bin.binary);

        std::fs::write(repo.join("crlf.txt"), gbk("甲\r\n乙\r\n")).expect("crlf file");
        write_worktree_file(&repo, "crlf.txt", "甲\n丙").expect("write back");
        assert_eq!(std::fs::read(repo.join("crlf.txt")).expect("read back"), gbk("甲\r\n丙\r\n"));
        assert!(write_worktree_file(&repo, "../escape.txt", "x").is_err());

        let committed = rev_diff_files(&repo, "test", "HEAD").expect("commit range");
        assert!(committed.iter().all(|file| file.path != "fresh.txt" && file.path != "cn.txt"));

        std::fs::remove_dir_all(repo).expect("remove fixture");
    }
}
