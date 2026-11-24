use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_SEARCH_FILE_SIZE: u64 = 2 * 1024 * 1024;
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    ".next",
    ".nuxt",
    "coverage",
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_max_results() -> usize {
    200
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub truncated: bool,
}

fn canonical_root(repo: &Path) -> Result<PathBuf, String> {
    let root = fs::canonicalize(repo).map_err(|error| format!("仓库目录无效：{error}"))?;
    if !root.is_dir() {
        return Err("仓库路径不是目录".to_string());
    }
    Ok(root)
}

fn validate_relative(rel: &Path, allow_empty: bool) -> Result<(), String> {
    if rel.as_os_str().is_empty() {
        return if allow_empty {
            Ok(())
        } else {
            Err("路径不能为空".to_string())
        };
    }
    if rel.is_absolute() {
        return Err("不允许绝对路径".to_string());
    }
    for component in rel.components() {
        match component {
            Component::Normal(part) => {
                if part.to_string_lossy().eq_ignore_ascii_case(".git") {
                    return Err("不允许访问 .git".to_string());
                }
            }
            _ => return Err("路径只能包含安全的相对路径片段".to_string()),
        }
    }
    Ok(())
}

fn relative_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn resolve_existing(
    repo: &Path,
    rel: &Path,
    allow_empty: bool,
) -> Result<(PathBuf, PathBuf), String> {
    validate_relative(rel, allow_empty)?;
    let root = canonical_root(repo)?;
    let target =
        fs::canonicalize(root.join(rel)).map_err(|error| format!("路径不存在：{error}"))?;
    if !target.starts_with(&root) {
        return Err("路径越过仓库边界".to_string());
    }
    Ok((root, target))
}

fn resolve_new(repo: &Path, rel: &Path) -> Result<PathBuf, String> {
    validate_relative(rel, false)?;
    let root = canonical_root(repo)?;
    let target = root.join(rel);
    match fs::symlink_metadata(&target) {
        Ok(_) => return Err("目标路径已存在".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("无法检查目标路径：{error}")),
    }
    let parent = target
        .parent()
        .ok_or_else(|| "目标路径无父目录".to_string())?;
    let parent = fs::canonicalize(parent).map_err(|error| format!("父目录不存在：{error}"))?;
    if !parent.starts_with(&root) {
        return Err("路径越过仓库边界".to_string());
    }
    let name = target
        .file_name()
        .ok_or_else(|| "目标名称无效".to_string())?;
    Ok(parent.join(name))
}

pub fn validate_terminal_cwd(repo: &Path, cwd: &Path) -> Result<PathBuf, String> {
    let root = canonical_root(repo)?;
    let target = fs::canonicalize(cwd).map_err(|error| format!("终端工作目录无效：{error}"))?;
    if !target.is_dir() || !target.starts_with(&root) {
        return Err("终端工作目录必须位于仓库内".to_string());
    }
    Ok(target)
}

pub fn list_dir(repo: &Path, rel: Option<&str>) -> Result<Vec<DirEntry>, String> {
    let rel = Path::new(rel.unwrap_or(""));
    let (root, target) = resolve_existing(repo, rel, true)?;
    if !target.is_dir() {
        return Err("目标路径不是目录".to_string());
    }

    let mut entries = Vec::new();
    for item in fs::read_dir(target).map_err(|error| format!("读取目录失败：{error}"))? {
        let item = item.map_err(|error| format!("读取目录项失败：{error}"))?;
        let file_type = item
            .file_type()
            .map_err(|error| format!("读取目录项类型失败：{error}"))?;
        let name = item.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case(".git") || file_type.is_symlink() {
            continue;
        }
        let path = item.path();
        let metadata = item
            .metadata()
            .map_err(|error| format!("读取目录项信息失败：{error}"))?;
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| "目录项越过仓库边界".to_string())?;
        entries.push(DirEntry {
            name,
            path: relative_string(relative),
            is_dir: file_type.is_dir(),
            size: if file_type.is_file() {
                metadata.len()
            } else {
                0
            },
        });
    }
    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(entries)
}

pub fn create_worktree_file(repo: &Path, rel: &str, content: &str) -> Result<(), String> {
    let target = resolve_new(repo, Path::new(rel))?;
    fs::write(target, content).map_err(|error| format!("创建文件失败：{error}"))
}

pub fn create_worktree_dir(repo: &Path, rel: &str) -> Result<(), String> {
    let target = resolve_new(repo, Path::new(rel))?;
    fs::create_dir(target).map_err(|error| format!("创建目录失败：{error}"))
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.replace('\\', "/");
    let value = value.replace('\\', "/");
    let mut expression = String::from("^");
    for character in pattern.chars() {
        match character {
            '*' => expression.push_str(".*"),
            '?' => expression.push('.'),
            other => expression.push_str(&regex::escape(&other.to_string())),
        }
    }
    expression.push('$');
    Regex::new(&expression)
        .map(|regex| regex.is_match(&value))
        .unwrap_or(false)
}

fn included(path: &str, patterns: &[String]) -> bool {
    patterns.is_empty()
        || patterns.iter().any(|pattern| {
            wildcard_match(pattern, path)
                || Path::new(path)
                    .file_name()
                    .map(|name| wildcard_match(pattern, &name.to_string_lossy()))
                    .unwrap_or(false)
        })
}

fn search_dir(
    root: &Path,
    dir: &Path,
    matcher: &Regex,
    options: &SearchOptions,
    result: &mut SearchResult,
) -> Result<(), String> {
    for item in fs::read_dir(dir).map_err(|error| format!("读取目录失败：{error}"))? {
        if result.hits.len() >= options.max_results {
            result.truncated = true;
            return Ok(());
        }
        let item = item.map_err(|error| format!("读取目录项失败：{error}"))?;
        let file_type = item
            .file_type()
            .map_err(|error| format!("读取目录项类型失败：{error}"))?;
        if file_type.is_symlink() {
            continue;
        }
        let path = item.path();
        if file_type.is_dir() {
            let name = item.file_name().to_string_lossy().into_owned();
            if !SKIPPED_DIRS
                .iter()
                .any(|skipped| name.eq_ignore_ascii_case(skipped))
            {
                search_dir(root, &path, matcher, options, result)?;
            }
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let relative = relative_string(
            path.strip_prefix(root)
                .map_err(|_| "搜索路径越界".to_string())?,
        );
        if !included(&relative, &options.include) {
            continue;
        }
        let metadata = item
            .metadata()
            .map_err(|error| format!("读取文件信息失败：{error}"))?;
        if metadata.len() > MAX_SEARCH_FILE_SIZE {
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        if bytes.contains(&0) {
            continue;
        }
        let content = match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(_) => continue,
        };
        for (line_index, line) in content.lines().enumerate() {
            for found in matcher.find_iter(line) {
                result.hits.push(SearchHit {
                    path: relative.clone(),
                    line: line_index + 1,
                    column: line[..found.start()].chars().count() + 1,
                    text: line.to_string(),
                });
                if result.hits.len() >= options.max_results {
                    result.truncated = true;
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

pub fn search_workspace(
    repo: &Path,
    query: &str,
    options: &SearchOptions,
) -> Result<SearchResult, String> {
    if query.is_empty() {
        return Err("搜索内容不能为空".to_string());
    }
    if options.max_results == 0 {
        return Ok(SearchResult {
            hits: Vec::new(),
            truncated: false,
        });
    }
    let root = canonical_root(repo)?;
    let pattern = if options.regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    let matcher = RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .build()
        .map_err(|error| format!("正则表达式无效：{error}"))?;
    let mut result = SearchResult {
        hits: Vec::new(),
        truncated: false,
    };
    search_dir(&root, &root, &matcher, options, &mut result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestRepo(PathBuf);

    impl TestRepo {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("rebased-workspace-{}-{unique}", std::process::id()));
            fs::create_dir_all(path.join(".git")).unwrap();
            fs::create_dir_all(path.join("src")).unwrap();
            fs::write(
                path.join("src/main.rs"),
                "fn main() {\n    println!(\"Hello\");\n}\n",
            )
            .unwrap();
            Self(path)
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn lists_lazily_and_hides_git() {
        let repo = TestRepo::new();
        let root = list_dir(&repo.0, None).unwrap();
        assert!(root.iter().any(|entry| entry.path == "src" && entry.is_dir));
        assert!(!root.iter().any(|entry| entry.name == ".git"));
        let src = list_dir(&repo.0, Some("src")).unwrap();
        assert_eq!(src.len(), 1);
        assert_eq!(src[0].path, "src/main.rs");
    }

    #[test]
    fn rejects_traversal_absolute_and_git_paths() {
        let repo = TestRepo::new();
        assert!(list_dir(&repo.0, Some("../")).is_err());
        assert!(list_dir(&repo.0, Some(&repo.0.to_string_lossy())).is_err());
        assert!(list_dir(&repo.0, Some(".git")).is_err());
        assert!(create_worktree_file(&repo.0, "../escape.txt", "").is_err());
    }

    #[test]
    fn rejects_symlink_escape() {
        let repo = TestRepo::new();
        let outside = repo.0.with_extension("outside");
        fs::create_dir_all(&outside).unwrap();
        let link = repo.0.join("outside-link");
        let file_link = repo.0.join("outside-file");
        let outside_file = outside.join("created-through-link.txt");

        #[cfg(unix)]
        let (linked, file_linked) = (
            std::os::unix::fs::symlink(&outside, &link).is_ok(),
            std::os::unix::fs::symlink(&outside_file, &file_link).is_ok(),
        );
        #[cfg(windows)]
        let (linked, file_linked) = (
            std::os::windows::fs::symlink_dir(&outside, &link).is_ok(),
            std::os::windows::fs::symlink_file(&outside_file, &file_link).is_ok(),
        );

        if linked {
            assert!(list_dir(&repo.0, Some("outside-link")).is_err());
        }
        if file_linked {
            assert!(create_worktree_file(&repo.0, "outside-file", "escape").is_err());
            assert!(!outside_file.exists());
        }
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn creates_files_and_directories() {
        let repo = TestRepo::new();
        create_worktree_dir(&repo.0, "src/generated").unwrap();
        create_worktree_file(&repo.0, "src/generated/mod.rs", "pub fn generated() {}").unwrap();
        assert_eq!(
            fs::read_to_string(repo.0.join("src/generated/mod.rs")).unwrap(),
            "pub fn generated() {}"
        );
    }

    #[test]
    fn searches_text_regex_and_skips_git() {
        let repo = TestRepo::new();
        fs::write(repo.0.join(".git/secret"), "Hello").unwrap();
        let options = SearchOptions {
            regex: true,
            case_sensitive: false,
            include: vec!["*.rs".to_string()],
            max_results: 10,
        };
        let result = search_workspace(&repo.0, "hello|println", &options).unwrap();
        assert_eq!(result.hits.len(), 2);
        assert!(result.hits.iter().all(|hit| hit.path == "src/main.rs"));
        assert!(result.hits.iter().all(|hit| hit.line == 2));
        assert_eq!(result.hits[0].column, 5);
    }

    #[test]
    fn enforces_result_limit_and_ignores_binary() {
        let repo = TestRepo::new();
        fs::write(repo.0.join("src/binary.bin"), b"Hello\0Hello").unwrap();
        fs::write(repo.0.join("src/two.txt"), "hello hello").unwrap();
        let options = SearchOptions {
            regex: false,
            case_sensitive: false,
            include: Vec::new(),
            max_results: 1,
        };
        let result = search_workspace(&repo.0, "hello", &options).unwrap();
        assert_eq!(result.hits.len(), 1);
        assert!(result.truncated);
        assert_ne!(result.hits[0].path, "src/binary.bin");
    }
}
