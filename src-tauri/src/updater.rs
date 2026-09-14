use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tauri::AppHandle;

pub const UPDATE_FEED_URL: &str = "http://111.230.247.111/rebased/latest.json";

#[derive(Deserialize)]
struct UpdateManifest {
    version: String,
    #[serde(default)]
    notes: String,
    url: String,
    #[serde(default)]
    sha256: String,
    #[serde(default)]
    silent_args: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub notes: String,
    pub url: String,
    pub available: bool,
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
    })
}

pub fn install_update(app: AppHandle) -> Result<String, String> {
    let manifest = fetch_manifest()?;
    if !is_newer(&manifest.version, &current_version()) {
        return Err("当前已是最新版本".to_string());
    }
    if manifest.url.trim().is_empty() {
        return Err("更新清单没有安装包地址".to_string());
    }
    let package = download_installer(&manifest)?;
    if is_setup_package(&manifest, &package) {
        spawn_silent(&package, manifest.silent_args.as_deref())?;
    } else {
        spawn_replace(&package)?;
    }
    app.exit(0);
    Ok("已启动更新，应用即将退出".to_string())
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
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(180)))
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
        .read_to_vec()
        .map_err(|e| format!("读取安装包失败：{e}"))?;
    if bytes.len() < 64 {
        return Err("安装包内容为空".to_string());
    }
    if !manifest.sha256.trim().is_empty() {
        let actual = hex_lower(&Sha256::digest(&bytes));
        if actual != manifest.sha256.trim().to_ascii_lowercase() {
            return Err("安装包校验失败，请重新发布更新".to_string());
        }
    }
    let ext = if manifest.url.to_ascii_lowercase().ends_with(".msi") {
        "msi"
    } else {
        "exe"
    };
    let path = std::env::temp_dir().join(format!("rebased-setup-{}.{ext}", manifest.version));
    let mut file = File::create(&path).map_err(|e| format!("无法保存安装包：{e}"))?;
    file.write_all(&bytes).map_err(|e| format!("写入安装包失败：{e}"))?;
    Ok(path)
}

fn is_setup_package(manifest: &UpdateManifest, path: &PathBuf) -> bool {
    let url = manifest.url.to_ascii_lowercase();
    let name = path
        .file_name()
        .and_then(|item| item.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    url.contains("setup") || url.ends_with(".msi") || name.contains("setup") || name.ends_with(".msi")
}

fn spawn_replace(new_exe: &PathBuf) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|e| format!("无法定位当前程序：{e}"))?;
    let bat = std::env::temp_dir().join("rebased-replace.bat");
    let script = format!(
        "@echo off\r\nping 127.0.0.1 -n 3 >nul\r\ncopy /y \"{}\" \"{}\"\r\nif errorlevel 1 exit /b 1\r\nstart \"\" \"{}\"\r\ndel \"%~f0\"\r\n",
        new_exe.display(),
        current.display(),
        current.display()
    );
    std::fs::write(&bat, script).map_err(|e| format!("无法写入更新脚本：{e}"))?;
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(&bat);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().map_err(|e| format!("无法启动更新替换：{e}"))?;
    Ok(())
}

fn spawn_silent(path: &PathBuf, silent_args: Option<&str>) -> Result<(), String> {
    let mut cmd = if path.extension().and_then(|e| e.to_str()).unwrap_or("").eq_ignore_ascii_case("msi") {
        let mut command = Command::new("msiexec");
        command.arg("/i").arg(path).arg("/qn").arg("/norestart");
        command
    } else {
        let mut command = Command::new(path);
        if let Some(args) = silent_args.filter(|item| !item.trim().is_empty()) {
            for arg in args.split_whitespace() {
                command.arg(arg);
            }
        } else {
            command.arg("/S");
        }
        command
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().map_err(|e| format!("无法启动静默安装：{e}"))?;
    Ok(())
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
