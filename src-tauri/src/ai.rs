use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::git::{self, AppState};
use crate::store::AppStore;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffPath {
    pub path: String,
    pub untracked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedMessage {
    pub message: String,
    pub source: String,
    pub model: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
}

pub fn generate_commit_message(
    state: &AppState,
    store: &AppStore,
    paths: &[DiffPath],
    repo: Option<&str>,
) -> Result<GeneratedMessage, String> {
    let repo = state.resolve_repo(repo)?;
    if paths.is_empty() {
        return Err("请先勾选要提交的文件".to_string());
    }
    let cfg = require_config(store)?;
    let (names, prompt) = collect_diff_context(&repo, paths);
    if prompt.trim().is_empty() {
        return Err("勾选的文件没有可读的文本变更，无法生成提交信息".to_string());
    }

    let message = call_chat_api(
        store,
        &cfg,
        "你是 Git 提交信息助手。根据用户提供的变更文件和 diff 写中文提交说明：第一行不超过 50 字，用 feat/fix/docs/refactor/chore/test 之一开头；必要时空一行后用短句列出要点。只输出提交信息，不要解释，不要用 markdown。",
        &format!("请根据以下 {} 个文件的实际差异生成提交信息。\n\n变更文件：\n{}\n\n差异：\n{}", names.len(), names.join("\n"), prompt),
    )?;
    Ok(GeneratedMessage {
        message,
        source: "ai".to_string(),
        model: cfg.model.clone(),
    })
}

pub fn test_ai_config(store: &AppStore) -> Result<String, String> {
    let cfg = require_config(store)?;
    let reply = call_chat_api(
        store,
        &cfg,
        "你只用于连通性测试。",
        "只回复一个词：ok",
    )?;
    Ok(format!(
        "已连通，模型 {}，接口返回：{}",
        cfg.model,
        reply.chars().take(40).collect::<String>()
    ))
}

pub fn get_ai_config(store: &AppStore) -> AiConfig {
    load_config(Some(store))
}

pub fn save_ai_config(store: &AppStore, config: AiConfig) -> Result<AiConfig, String> {
    let config = AiConfig {
        base_url: config.base_url.trim().to_string(),
        api_key: config.api_key.trim().to_string(),
        model: config.model.trim().to_string(),
    };
    if looks_like_url(&config.api_key) {
        return Err("API Key 不能填接口地址，请填写密钥；若服务不需要密钥请留空".to_string());
    }
    if config.base_url.is_empty() {
        return Err("请填写接口地址".to_string());
    }
    if config.model.is_empty() {
        return Err("请填写模型名称".to_string());
    }
    store.set_json("ai", &config)?;
    Ok(config)
}

fn require_config(store: &AppStore) -> Result<AiConfig, String> {
    let cfg = load_config(Some(store));
    if cfg.base_url.trim().is_empty() {
        return Err("未配置 AI 接口地址，请先在设置中填写".to_string());
    }
    if cfg.model.trim().is_empty() {
        return Err("未配置 AI 模型名称，请先在设置中填写".to_string());
    }
    if looks_like_url(&cfg.api_key) {
        return Err("当前 API Key 填成了网址。请到设置里改成真正的密钥；不需要密钥则留空".to_string());
    }
    Ok(cfg)
}

fn load_config(store: Option<&AppStore>) -> AiConfig {
    let mut cfg = AiConfig {
        base_url: std::env::var("REBASED_AI_BASE_URL").unwrap_or_default(),
        api_key: std::env::var("REBASED_AI_API_KEY").unwrap_or_default(),
        model: std::env::var("REBASED_AI_MODEL").unwrap_or_default(),
    };
    if let Some(store) = store {
        if let Ok(Some(file)) = store.get_json::<AiConfig>("ai") {
            merge_ai(&mut cfg, file);
        }
    }
    cfg
}

fn merge_ai(cfg: &mut AiConfig, file: AiConfig) {
    if cfg.base_url.is_empty() {
        cfg.base_url = file.base_url;
    }
    if cfg.api_key.is_empty() {
        cfg.api_key = file.api_key;
    }
    if cfg.model.is_empty() {
        cfg.model = file.model;
    }
}

fn collect_diff_context(repo: &std::path::Path, paths: &[DiffPath]) -> (Vec<String>, String) {
    let mut names = Vec::new();
    let mut snippets = Vec::new();
    let mut total = 0usize;
    for item in paths.iter().take(40) {
        names.push(item.path.clone());
        let diff = git::working_diff(repo, &item.path, item.untracked).unwrap_or_else(|_| git::FileDiff {
            path: item.path.clone(),
            binary: false,
            patch: String::new(),
        });
        let body = if diff.binary {
            format!("文件：{}\n（二进制，已省略内容）", item.path)
        } else if diff.patch.trim().is_empty() {
            format!("文件：{}\n（没有文本 diff，请根据文件名理解变更）", item.path)
        } else {
            let mut patch = diff.patch;
            if truncate_utf8(&mut patch, 4000) {
                patch.push_str("\n…");
            }
            format!("文件：{}\n{}", item.path, patch)
        };
        total += body.len();
        snippets.push(body);
        if total > 24000 {
            snippets.push("（后续文件已省略，避免超出模型上下文）".to_string());
            break;
        }
    }
    (names, snippets.join("\n\n"))
}

fn call_chat_api(store: &AppStore, cfg: &AiConfig, system: &str, user: &str) -> Result<String, String> {
    let _ = store;
    let url = chat_completions_url(&cfg.base_url);
    let body = serde_json::json!({
        "model": cfg.model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ]
    });

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(90)))
        .build()
        .into();

    let mut request = agent
        .post(&url)
        .header("Content-Type", "application/json");
    if !cfg.api_key.is_empty() {
        request = request.header("Authorization", format!("Bearer {}", cfg.api_key));
    }

    let mut response = request
        .send_json(body)
        .map_err(|e| format!("无法连接 AI 接口 {url}：{e}"))?;
    let status = u16::from(response.status());
    let value: serde_json::Value = response
        .body_mut()
        .read_json()
        .map_err(|e| format!("AI 接口返回无法解析：{e}"))?;
    if !(200..300).contains(&status) {
        return Err(format_api_error(status, &value, &url, cfg));
    }
    let text = extract_content(&value).ok_or_else(|| {
        format!(
            "AI 未返回提交信息。接口：{}，模型：{}",
            url, cfg.model
        )
    })?;
    Ok(clean_message(&text))
}

fn chat_completions_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    }
}

fn extract_content(value: &serde_json::Value) -> Option<String> {
    let message = &value["choices"][0]["message"];
    if let Some(text) = message["content"].as_str() {
        let text = text.trim();
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }
    if let Some(parts) = message["content"].as_array() {
        let text = parts
            .iter()
            .filter_map(|part| part["text"].as_str().or_else(|| part.as_str()))
            .collect::<Vec<_>>()
            .join("");
        if !text.trim().is_empty() {
            return Some(text.trim().to_string());
        }
    }
    if let Some(text) = value["choices"][0]["text"].as_str() {
        if !text.trim().is_empty() {
            return Some(text.trim().to_string());
        }
    }
    None
}

fn clean_message(text: &str) -> String {
    let trimmed = text.trim();
    let stripped = trimmed
        .strip_prefix("```")
        .and_then(|rest| rest.strip_suffix("```"))
        .map(|rest| {
            let rest = rest.trim_start();
            rest.strip_prefix("text\n")
                .or_else(|| rest.strip_prefix("markdown\n"))
                .unwrap_or(rest)
                .trim()
        })
        .unwrap_or(trimmed);
    stripped.trim().to_string()
}

fn format_api_error(status: u16, value: &serde_json::Value, url: &str, cfg: &AiConfig) -> String {
    let detail = value["error"]["message"]
        .as_str()
        .or_else(|| value["error"].as_str())
        .or_else(|| value["message"].as_str())
        .unwrap_or("无详细信息");
    match status {
        401 | 403 => format!(
            "AI 接口未授权（HTTP {status}）。请检查 API Key。当前模型 {}，接口 {}",
            cfg.model, url
        ),
        404 => format!("AI 接口不存在（HTTP 404）：{url}，请确认地址是否包含 /v1"),
        _ => format!("AI 接口错误 HTTP {status}：{detail}（模型 {}，接口 {}）", cfg.model, url),
    }
}

fn truncate_utf8(text: &mut String, max_bytes: usize) -> bool {
    if text.len() <= max_bytes {
        return false;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    true
}

fn looks_like_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}
