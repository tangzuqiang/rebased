use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

pub const UPDATE_FEED_URL: &str = "http://111.230.247.111/rebased/latest.json";

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

pub fn install_update() -> Result<String, String> {
    let manifest = fetch_manifest()?;
    if !is_newer(&manifest.version, &current_version()) {
        return Err("当前已是最新版本".to_string());
    }
    if manifest.url.trim().is_empty() {
        return Err("更新清单没有安装包地址".to_string());
    }

    let package = download_installer(&manifest)?;
    open_installer(&package)?;
    Ok("安装包已打开，请按安装向导完成更新".to_string())
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
    file.write_all(&bytes)
        .map_err(|e| format!("写入安装包失败：{e}"))?;
    Ok(path)
}

fn open_installer(package: &PathBuf) -> Result<(), String> {
    let is_msi = package
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("msi"));

    let mut command = if is_msi {
        let mut command = Command::new("msiexec.exe");
        command.arg("/i").arg(package);
        command
    } else {
        Command::new(package)
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法打开安装包：{e}"))
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
