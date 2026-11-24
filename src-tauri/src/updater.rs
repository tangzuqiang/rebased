use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::AppHandle;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub const UPDATE_FEED_URL: &str = "http://111.230.247.111/rebased/latest.json";
const SIGNER_THUMBPRINT: &str = include_str!("../signing/thumbprint.txt");
const SILENT_ARGS: [&str; 3] = ["/S", "/R", "/UPDATE"];
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

static INSTALLING: AtomicBool = AtomicBool::new(false);

#[derive(Deserialize)]
struct UpdateManifest {
    version: String,
    #[serde(default)]
    notes: String,
    url: String,
    #[serde(default)]
    sha256: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub notes: String,
    pub url: String,
    pub available: bool,
    pub auto_install: bool,
}

pub fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn check_update() -> Result<UpdateInfo, String> {
    let manifest = fetch_manifest()?;
    Ok(UpdateInfo {
        current: current_version(),
        latest: manifest.version.clone(),
        notes: manifest.notes,
        url: manifest.url,
        available: is_newer(&manifest.version, &current_version()),
        auto_install: !cfg!(debug_assertions),
    })
}

pub fn install_update(app: AppHandle) -> Result<String, String> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("更新正在进行中".to_string());
    }
    let result = prepare_and_launch();
    match &result {
        Ok(_) => {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(1500));
                app.exit(0);
            });
        }
        Err(_) => INSTALLING.store(false, Ordering::SeqCst),
    }
    result
}

fn prepare_and_launch() -> Result<String, String> {
    let manifest = fetch_manifest()?;
    if !is_newer(&manifest.version, &current_version()) {
        return Err("当前已是最新版本".to_string());
    }
    if manifest.url.trim().is_empty() {
        return Err("更新清单没有安装包地址".to_string());
    }

    let package = download_installer(&manifest)?;
    if let Err(error) = verify_signature(&package) {
        let _ = std::fs::remove_file(&package);
        return Err(error);
    }
    launch_silent(&package)?;
    Ok(format!(
        "正在静默安装 {}，完成后会自动重新打开",
        manifest.version
    ))
}

fn fetch_manifest() -> Result<UpdateManifest, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut response = agent
        .get(UPDATE_FEED_URL)
        .call()
        .map_err(|e| format!("无法连接更新服务器：{e}"))?;
    let status = u16::from(response.status());
    if !(200..300).contains(&status) {
        return Err(format!("更新服务器返回 {status}"));
    }
    response
        .body_mut()
        .read_json::<UpdateManifest>()
        .map_err(|e| format!("更新清单无法解析：{e}"))
}

fn download_installer(manifest: &UpdateManifest) -> Result<PathBuf, String> {
    if !manifest.url.to_ascii_lowercase().ends_with(".exe") {
        return Err("静默更新只支持 NSIS 安装包（*-setup.exe）".to_string());
    }
    if manifest.sha256.trim().is_empty() {
        return Err("更新清单缺少 sha256，已拒绝安装".to_string());
    }

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(300)))
        .build()
        .into();
    let mut response = agent
        .get(&manifest.url)
        .call()
        .map_err(|e| format!("下载安装包失败：{e}"))?;
    let status = u16::from(response.status());
    if !(200..300).contains(&status) {
        return Err(format!("下载安装包失败，状态码 {status}"));
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(512 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("读取安装包失败：{e}"))?;
    if bytes.len() < 64 {
        return Err("安装包内容为空".to_string());
    }
    let actual = hex_lower(Sha256::digest(&bytes));
    if actual != manifest.sha256.trim().to_ascii_lowercase() {
        return Err("安装包校验失败，请重新发布更新".to_string());
    }

    let path = std::env::temp_dir().join(format!("rebased-setup-{}.exe", manifest.version));
    let mut file = File::create(&path).map_err(|e| format!("无法保存安装包：{e}"))?;
    file.write_all(&bytes)
        .map_err(|e| format!("写入安装包失败：{e}"))?;
    Ok(path)
}

/// Self-signed certificates never chain to a trusted root, so Windows reports
/// `UnknownError` for an intact file; tampering shows up as `HashMismatch`.
#[cfg(windows)]
fn verify_signature(package: &Path) -> Result<(), String> {
    let expected = SIGNER_THUMBPRINT.trim().to_ascii_uppercase();
    if expected.is_empty() {
        return Err("客户端未内置签名证书指纹，已拒绝安装".to_string());
    }
    let script = "$s = Get-AuthenticodeSignature -LiteralPath $env:REBASED_UPDATE_FILE; \
                  Write-Output (\"{0}|{1}\" -f $s.Status, $s.SignerCertificate.Thumbprint)";
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .env("REBASED_UPDATE_FILE", package)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("无法校验安装包签名：{e}"))?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let (status, thumbprint) = text.split_once('|').unwrap_or((text.as_str(), ""));
    if !matches!(status, "Valid" | "UnknownError") {
        return Err(format!("安装包签名无效（{status}），已拒绝安装"));
    }
    if !thumbprint.trim().eq_ignore_ascii_case(&expected) {
        return Err("安装包签名证书不匹配，已拒绝安装".to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
fn verify_signature(_package: &Path) -> Result<(), String> {
    Err("当前平台不支持自动更新".to_string())
}

fn launch_silent(package: &Path) -> Result<(), String> {
    let mut command = Command::new(package);
    command.args(SILENT_ARGS);
    #[cfg(windows)]
    command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法启动安装程序：{e}"))
}

fn is_newer(latest: &str, current: &str) -> bool {
    parse_version(latest) > parse_version(current)
}

fn parse_version(value: &str) -> [u32; 3] {
    let mut parts = value
        .trim()
        .trim_start_matches('v')
        .split(|c: char| !c.is_ascii_digit())
        .filter(|item| !item.is_empty())
        .filter_map(|item| item.parse::<u32>().ok());
    [
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    ]
}

fn hex_lower(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}
