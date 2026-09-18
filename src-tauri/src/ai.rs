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
    let (names, prompt) = collect_diff_context(&repo, paths)?;
    if prompt.trim().is_empty() {
        return Err("勾选的文件没有可读的文本变更，无法生成提交信息".to_string());
    }

    let mut message = call_chat_api(
        store,
        &cfg,
        COMMIT_SYSTEM_PROMPT,
        &format!(
            "请根据以下 {} 个文件的实际差异生成详细提交信息。\n\n变更文件：\n{}\n\n差异：\n{}\n\n最后要求：必须用简体中文；第一行是类型前缀加中文标题；空一行后写详细正文，按文件或改动点分条说明改了什么、为什么改。不要只写一行。禁止英文句子。",
            names.len(),
            names.join("\n"),
            prompt
        ),
        ChatOptions {
            think: false,
            max_tokens: 2048,
        },
    )?;
    if looks_mostly_english(&message) {
        message = call_chat_api(
            store,
            &cfg,
            "你只负责把 Git 提交信息译成简体中文。保留 feat/fix/docs/refactor/chore/test 前缀和原有段落、列表结构，不要压缩成一行。只输出译文，不要解释。",
            &format!("请译成简体中文，保持详细程度：\n{message}"),
            ChatOptions {
                think: false,
                max_tokens: 1200,
            },
        )?;
    }
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
        ChatOptions {
            think: false,
            max_tokens: 16,
        },
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

const REQUEST_TIMEOUT_SECS: u64 = 240;
const COMMIT_SYSTEM_PROMPT: &str = "\
你是 Git 提交信息助手。必须用简体中文写出详细提交说明，禁止整段英文。\
第一行不超过 50 个汉字，格式为「类型: 中文摘要」，类型只能是 feat、fix、docs、refactor、chore、test 之一。\
空一行后写正文：用 3 到 8 条中文短句说明每个关键改动、影响范围和原因，不要只输出一行标题。\
只输出提交信息，不要解释，不要用 markdown 代码块。\
示例：\
feat: 为 Ollama 请求关闭思考并保持模型常驻\n\n\
- 增加 keep_alive=-1，避免每次重新加载模型\n\
- 生成提交信息时允许短思考，并提高输出长度以免正文被截断\n\
- 若结果仍是英文，再翻译成中文并保留条目结构";

struct ChatOptions {
    think: bool,
    max_tokens: u32,
}

#[derive(Clone, Copy)]
enum ThinkMode {
    On,
    Off,
    Omit,
}

fn collect_diff_context(repo: &std::path::Path, paths: &[DiffPath]) -> Result<(Vec<String>, String), String> {
    let mut names = Vec::new();
    let mut snippets = Vec::new();
    for item in paths {
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
            format!("文件：{}\n{}", item.path, diff.patch)
        };
        snippets.push(body);
    }
    Ok((names, snippets.join("\n\n")))
}

fn is_context_overflow(status: u16, detail: &str) -> bool {
    if status == 413 {
        return true;
    }
    let lower = detail.to_ascii_lowercase();
    lower.contains("too long")
        || lower.contains("context length")
        || lower.contains("context window")
        || lower.contains("maximum context")
        || lower.contains("too many tokens")
        || lower.contains("prompt is too long")
        || lower.contains("content too large")
        || lower.contains("exceeds") && lower.contains("context")
}

fn call_chat_api(
    store: &AppStore,
    cfg: &AiConfig,
    system: &str,
    user: &str,
    opts: ChatOptions,
) -> Result<String, String> {
    let _ = store;
    let url = chat_completions_url(&cfg.base_url);
    let mut think_mode = think_mode_for(&cfg.model, opts.think);
    let mut retried_empty = false;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(REQUEST_TIMEOUT_SECS)))
        .build()
        .into();

    loop {
        let body = chat_request_body(cfg, system, user, opts.max_tokens, think_mode);
        let mut request = agent
            .post(&url)
            .header("Content-Type", "application/json");
        if !cfg.api_key.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", cfg.api_key));
        }
        let mut response = request.send_json(body).map_err(|e| {
            let err = e.to_string();
            let lower = err.to_ascii_lowercase();
            if lower.contains("timed out") || lower.contains("timeout") {
                format!(
                    "模型推理超时（已等待 4 分钟）。这是本地模型读 diff、生成文本偏慢，把文件压成二进制不会更快，模型也读不懂压缩内容。可换更快的小模型、少勾选几个文件，或给 Ollama 足够 CPU/GPU。接口 {url}"
                )
            } else {
                format!("无法连接 AI 接口 {url}：{err}")
            }
        })?;
        let status = u16::from(response.status());
        let value: serde_json::Value = response
            .body_mut()
            .read_json()
            .map_err(|e| format!("AI 接口返回无法解析：{e}"))?;
        if !matches!(think_mode, ThinkMode::Omit) && status == 400 && thinking_not_supported(&value) {
            think_mode = ThinkMode::Omit;
            continue;
        }
        if !(200..300).contains(&status) {
            return Err(format_api_error(status, &value, &url, cfg));
        }
        if let Some(text) = extract_content(&value) {
            return Ok(clean_message(&text));
        }
        let finish = value["choices"][0]["finish_reason"]
            .as_str()
            .unwrap_or("unknown");
        if !retried_empty && finish == "length" && matches!(think_mode, ThinkMode::On) {
            think_mode = ThinkMode::Off;
            retried_empty = true;
            continue;
        }
        return Err(format!(
            "AI 未返回提交信息（finish_reason={finish}）。接口：{}，模型：{}。推理模型若把额度用在思考上，正文会为空。",
            url, cfg.model
        ));
    }
}

fn chat_request_body(
    cfg: &AiConfig,
    system: &str,
    user: &str,
    max_tokens: u32,
    think_mode: ThinkMode,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": cfg.model,
        "keep_alive": -1,
        "temperature": 0.3,
        "max_tokens": max_tokens,
        "stream": false,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ]
    });
    match think_mode {
        ThinkMode::On => {
            body["think"] = serde_json::json!(true);
            body["reasoning_effort"] = serde_json::json!("low");
        }
        ThinkMode::Off => {
            body["think"] = serde_json::json!(false);
            body["reasoning_effort"] = serde_json::json!("none");
        }
        ThinkMode::Omit => {}
    }
    body
}

fn think_mode_for(model: &str, want_think: bool) -> ThinkMode {
    if !model_supports_thinking(model) {
        ThinkMode::Omit
    } else if want_think {
        ThinkMode::On
    } else {
        ThinkMode::Off
    }
}

fn model_supports_thinking(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.contains("qwen3")
        || model.contains("deepseek-r1")
        || model.contains("qwq")
        || model.contains("gpt-oss")
        || model.contains("thinking")
}

fn thinking_not_supported(value: &serde_json::Value) -> bool {
    let detail = value["error"]["message"]
        .as_str()
        .or_else(|| value["error"].as_str())
        .or_else(|| value["message"].as_str())
        .unwrap_or("");
    let lower = detail.to_ascii_lowercase();
    lower.contains("does not support thinking") || detail.contains("不支持思考")
}

fn chat_completions_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        return base.to_string();
    }
    if base.ends_with("/v1") || base.contains("/v1/") {
        return format!("{base}/chat/completions");
    }
    format!("{base}/v1/chat/completions")
}

fn extract_content(value: &serde_json::Value) -> Option<String> {
    let message = &value["choices"][0]["message"];
    first_text(&message["content"])
        .or_else(|| first_text(&value["choices"][0]["text"]))
        .and_then(|text| {
            let text = strip_think_tags(&text);
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
}

fn strip_think_tags(text: &str) -> String {
    let mut rest = text;
    let mut out = String::new();
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("</think>") {
            rest = &rest[start + end + "</think>".len()..];
        } else {
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

fn first_text(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        let text = text.trim();
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }
    if let Some(parts) = value.as_array() {
        let text = parts
            .iter()
            .filter_map(|part| part["text"].as_str().or_else(|| part.as_str()))
            .collect::<Vec<_>>()
            .join("");
        let text = text.trim();
        if !text.is_empty() {
            return Some(text.to_string());
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
        404 => format!(
            "AI 接口返回 404：{detail}。当前模型「{}」，接口 {}。模型名必须和 ollama list 完全一致（例如 qwen3:1.7b、gemma-4-31B-it），不能简写成 qw3；若模型本身可用，再检查地址是否为 /v1/chat/completions",
            cfg.model, url
        ),
        _ if is_context_overflow(status, detail) => {
            format!("勾选文件的差异过大，模型上下文放不下（HTTP {status}）：{detail}。请减少勾选文件后重试。")
        }
        _ => format!("AI 接口错误 HTTP {status}：{detail}（模型 {}，接口 {}）", cfg.model, url),
    }
}

fn looks_like_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn looks_mostly_english(text: &str) -> bool {
    let mut cjk = 0usize;
    let mut letters = 0usize;
    for ch in text.chars() {
        if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
            cjk += 1;
        } else if ch.is_ascii_alphabetic() {
            letters += 1;
        }
    }
    cjk == 0 && letters > 8 || letters > cjk.saturating_mul(3) && letters > 12
}
