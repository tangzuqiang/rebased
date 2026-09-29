use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
    spawn_updater(&package, manifest.silent_args.as_deref())?;
    app.exit(0);
    Ok("已启动更新，应用即将退出并自动重新打开".to_string())
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

fn looks_like_installer(path: &PathBuf) -> bool {
    if path
        .extension()
        .and_then(|item| item.to_str())
        .unwrap_or("")
        .eq_ignore_ascii_case("msi")
    {
        return true;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes.windows(12).any(|chunk| chunk == b"NullsoftInst")
}

fn ps_single(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn nsis_arguments(silent_args: Option<&str>, install_dir: &str) -> String {
    let mut args: Vec<String> = silent_args
        .unwrap_or("/S")
        .split_whitespace()
        .filter(|item| {
            let lower = item.to_ascii_lowercase();
            !lower.starts_with("/d=") && lower != "/r"
        })
        .map(|item| item.to_string())
        .collect();
    if !args.iter().any(|item| item.eq_ignore_ascii_case("/S")) {
        args.insert(0, "/S".to_string());
    }
    let dir = install_dir.trim_end_matches(['\\', '/']);
    args.push(format!("/D={dir}"));
    args.join(" ")
}

fn spawn_updater(package: &PathBuf, silent_args: Option<&str>) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|e| format!("无法定位当前程序：{e}"))?;
    let install_dir = current
        .parent()
        .map(|path| path.to_string_lossy().to_string())
        .filter(|path| !path.is_empty())
        .ok_or_else(|| "无法确定安装目录".to_string())?;
    let pid = std::process::id();
    let installer = looks_like_installer(package);
    let nsis_args = nsis_arguments(silent_args, &install_dir);
    let script = format!(
        "$ErrorActionPreference = 'Continue'\n\
         $oldPid = {pid}\n\
         $package = {}\n\
         $current = {}\n\
         $installDir = {}\n\
         $isInstaller = {}\n\
         $nsisArgs = {}\n\
         $log = Join-Path $env:TEMP 'rebased-update.log'\n\
         function Log($m) {{ Add-Content -LiteralPath $log -Value (\"$(Get-Date -Format o) $m\") }}\n\
         Log \"start pid=$oldPid installer=$isInstaller dir=$installDir\"\n\
         $deadline = (Get-Date).AddSeconds(90)\n\
         while (Get-Process -Id $oldPid -ErrorAction SilentlyContinue) {{\n\
           if ((Get-Date) -gt $deadline) {{ Log 'timeout waiting old process'; break }}\n\
           Start-Sleep -Milliseconds 300\n\
         }}\n\
         Start-Sleep -Milliseconds 800\n\
         if ($isInstaller -and (Test-Path -LiteralPath $package)) {{\n\
           Log \"run installer $package -> $installDir\"\n\
           $psi = New-Object System.Diagnostics.ProcessStartInfo\n\
           $psi.UseShellExecute = $false\n\
           $psi.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden\n\
           if ([IO.Path]::GetExtension($package) -ieq '.msi') {{\n\
             $q = [char]34\n\
             $psi.FileName = 'msiexec.exe'\n\
             $psi.Arguments = '/i ' + $q + $package + $q + ' /qn /norestart INSTALLDIR=' + $q + $installDir + $q\n\
           }} else {{\n\
             $psi.FileName = $package\n\
             $psi.Arguments = $nsisArgs\n\
           }}\n\
           $proc = [System.Diagnostics.Process]::Start($psi)\n\
           if ($proc) {{\n\
             [void]$proc.WaitForExit(180000)\n\
             Log \"installer exit $($proc.ExitCode)\"\n\
           }} else {{\n\
             Log 'installer did not start'\n\
           }}\n\
         }} elseif (Test-Path -LiteralPath $package) {{\n\
           foreach ($i in 1..8) {{\n\
             try {{\n\
               Copy-Item -LiteralPath $package -Destination $current -Force\n\
               Log \"copied to $current\"\n\
               break\n\
             }} catch {{\n\
               Log \"copy failed $current $($_.Exception.Message)\"\n\
               Start-Sleep -Milliseconds 400\n\
             }}\n\
           }}\n\
         }}\n\
         Start-Sleep -Milliseconds 800\n\
         if (Get-Process -Name 'rebased','Rebased' -ErrorAction SilentlyContinue) {{\n\
           Log 'already running'\n\
           exit 0\n\
         }}\n\
         $leaf = Split-Path -Leaf $current\n\
         $candidates = @($current, (Join-Path $installDir $leaf), (Join-Path $installDir 'rebased.exe'), (Join-Path $installDir 'Rebased.exe'))\n\
         foreach ($app in $candidates) {{\n\
           if (Test-Path -LiteralPath $app) {{\n\
             Log \"start $app\"\n\
             Start-Process -FilePath $app\n\
             exit 0\n\
           }}\n\
         }}\n\
         Log 'no exe to start'\n",
        ps_single(&package.to_string_lossy()),
        ps_single(&current.to_string_lossy()),
        ps_single(&install_dir),
        if installer { "$true" } else { "$false" },
        ps_single(&nsis_args)
    );
    let ps1 = std::env::temp_dir().join("rebased-update.ps1");
    std::fs::write(&ps1, script).map_err(|e| format!("无法写入更新脚本：{e}"))?;
    launch_orphaned_powershell(&ps1)?;
    std::thread::sleep(Duration::from_millis(300));
    Ok(())
}

fn hide_window(cmd: &mut Command) {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
}

fn orphan_via_cim(command_line: &str) -> Result<(), String> {
    let escaped = command_line.replace('\'', "''");
    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &format!(
            "$r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{{CommandLine='{escaped}'}}; if ($r.ReturnValue -ne 0) {{ exit 1 }}"
        ),
    ]);
    hide_window(&mut cmd);
    let status = cmd.status().map_err(|e| format!("无法通过 WMI 启动更新：{e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("WMI 创建更新进程失败".to_string())
    }
}

fn orphan_via_wmic(command_line: &str) -> Result<(), String> {
    let mut cmd = Command::new("wmic");
    cmd.args(["process", "call", "create", command_line]);
    hide_window(&mut cmd);
    let status = cmd.status().map_err(|e| format!("无法通过 WMIC 启动更新：{e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("WMIC 创建更新进程失败".to_string())
    }
}

fn launch_orphaned_powershell(script: &Path) -> Result<(), String> {
    let command_line = format!(
        "powershell.exe -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File \"{}\"",
        script.display()
    );
    if orphan_via_cim(&command_line).is_ok() || orphan_via_wmic(&command_line).is_ok() {
        return Ok(());
    }
    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-WindowStyle",
        "Hidden",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        &script.to_string_lossy(),
    ]);
    hide_window(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0100_0008 | 0x0000_0200 | 0x0800_0000);
    }
    cmd.spawn().map_err(|e| format!("无法启动更新进程：{e}"))?;
    std::thread::sleep(Duration::from_millis(400));
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

#[cfg(test)]
mod tests {
    use super::nsis_arguments;

    #[test]
    fn silent_install_targets_the_running_directory() {
        let args = nsis_arguments(Some("/S /R"), r"D:\Apps\Rebased\");
        assert_eq!(args, r"/S /D=D:\Apps\Rebased");
    }

    #[test]
    fn install_dir_with_spaces_stays_last_and_unquoted() {
        let args = nsis_arguments(None, r"C:\Program Files\Rebased");
        assert_eq!(args, r"/S /D=C:\Program Files\Rebased");
    }
}
