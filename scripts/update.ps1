param(
    [switch]$SkipBuild,
    [switch]$SkipPublish
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$root = Split-Path -Parent $PSScriptRoot
$updateFile = Join-Path $root "update.txt"
$envFile = Join-Path $PSScriptRoot ".update-server.env"
$utf8 = New-Object System.Text.UTF8Encoding $false

function Write-Utf8File([string]$Path, [string]$Content) {
    [System.IO.File]::WriteAllText($Path, $Content, $utf8)
}

function Read-UpdateFile([string]$Path) {
    if (-not (Test-Path $Path)) {
        throw "找不到 $Path，请按「版本号 更新日志」逐行记录，脚本只读取最后一行"
    }

    $last = $null
    foreach ($line in [System.IO.File]::ReadAllLines($Path, [System.Text.UTF8Encoding]::new($false))) {
        $trim = $line.TrimStart([char]0xFEFF).Trim()
        if ($trim -eq "" -or $trim.StartsWith("#")) {
            continue
        }
        $last = $trim
    }

    if (-not $last) {
        throw "update.txt 没有有效记录，请追加一行：0.1.1 本次更新说明"
    }
    if ($last -notmatch "^(\d+\.\d+\.\d+)\s+(.+)$") {
        throw "最后一行格式不正确：$last`n请写成：版本号 更新日志    例如 0.1.1 修复分支切换"
    }

    return [pscustomobject]@{
        Version = $Matches[1]
        Notes   = $Matches[2].Trim()
    }
}

function Set-FirstMatch([string]$Path, [string]$Pattern, [string]$Replacement, [string]$Version) {
    $content = [System.IO.File]::ReadAllText($Path)
    $updated = [regex]::Replace($content, $Pattern, $Replacement, 1)
    if ($updated -eq $content -and $content -notmatch [regex]::Escape($Version)) {
        throw "未能更新版本号：$Path"
    }
    Write-Utf8File $Path $updated
}

function Sync-ProjectVersion([string]$Version) {
    Set-FirstMatch (Join-Path $root "src-tauri\Cargo.toml") '(?m)^version\s*=\s*"[^"]+"' "version = `"$Version`"" $Version
    Set-FirstMatch (Join-Path $root "src-tauri\tauri.conf.json") '("version"\s*:\s*")[^"]+"' "`${1}$Version`"" $Version
    Set-FirstMatch (Join-Path $root "package.json") '("version"\s*:\s*")[^"]+"' "`${1}$Version`"" $Version
}

function Find-Installer([string]$Version) {
    $nsis = Join-Path $root "src-tauri\target\release\bundle\nsis"
    $exact = Join-Path $nsis "Rebased_${Version}_x64-setup.exe"
    if (Test-Path $exact) {
        return (Get-Item $exact)
    }
    if (Test-Path $nsis) {
        $found = Get-ChildItem $nsis -Filter "*${Version}*setup.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if ($found) {
            return $found
        }
    }
    return $null
}

function Import-UpdateEnv {
    if (Test-Path $envFile) {
        Get-Content $envFile | ForEach-Object {
            if ($_ -match "^\s*#" -or $_ -notmatch "=") { return }
            $pair = $_ -split "=", 2
            Set-Item -Path "Env:$($pair[0].Trim())" -Value $pair[1].Trim()
        }
    }
}

function Publish-Installer([string]$Installer, [string]$Version, [string]$Notes) {
    Import-UpdateEnv

    $hostName = if ($env:UPDATE_HOST) { $env:UPDATE_HOST } else { "111.230.247.111" }
    $user = if ($env:UPDATE_USER) { $env:UPDATE_USER } else { "root" }
    $pass = $env:UPDATE_PASS
    $remoteDir = if ($env:UPDATE_DIR) { $env:UPDATE_DIR } else { "/usr/share/nginx/html/rebased" }

    if (-not $pass) {
        throw "请先在 scripts/.update-server.env 中填写 UPDATE_PASS（该文件已加入 gitignore）"
    }
    if (-not (Test-Path $Installer)) {
        throw "找不到安装包：$Installer"
    }

    $exe = Get-Item $Installer
    $hash = (Get-FileHash -Algorithm SHA256 $exe.FullName).Hash.ToLower()
    $fileName = $exe.Name
    $feedUrl = "http://$hostName/rebased/$fileName"
    $manifest = @{
        version    = $Version
        notes      = $Notes
        url        = $feedUrl
        sha256     = $hash
        silentArgs = "/S"
    } | ConvertTo-Json -Compress
    $manifestPath = Join-Path $env:TEMP "rebased-latest.json"
    Write-Utf8File $manifestPath $manifest

    Write-Host "上传 $fileName 和 latest.json 到 ${user}@${hostName}:$remoteDir"

    $py = @'
import os, posixpath, sys
try:
    import paramiko
except ImportError:
    sys.exit("需要先安装：pip install paramiko")

host = os.environ["UPDATE_HOST"]
user = os.environ["UPDATE_USER"]
password = os.environ["UPDATE_PASS"]
remote_dir = os.environ["UPDATE_DIR"]
local_installer = sys.argv[1]
local_manifest = sys.argv[2]
remote_name = sys.argv[3]

client = paramiko.SSHClient()
client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
client.connect(host, username=user, password=password, timeout=20)
sftp = client.open_sftp()
try:
    sftp.mkdir(remote_dir)
except OSError:
    pass
sftp.put(local_installer, posixpath.join(remote_dir, remote_name))
sftp.put(local_manifest, posixpath.join(remote_dir, "latest.json"))
sftp.close()
client.close()
print("已发布", remote_name)
'@

    $env:UPDATE_HOST = $hostName
    $env:UPDATE_USER = $user
    $env:UPDATE_PASS = $pass
    $env:UPDATE_DIR = $remoteDir
    python -c $py $exe.FullName $manifestPath $fileName
    Write-Host "清单地址：http://$hostName/rebased/latest.json"
}

$info = Read-UpdateFile $updateFile
Write-Host "读取 update.txt：版本 $($info.Version)"
Write-Host "更新日志："
Write-Host $info.Notes
Write-Host ""

Write-Host "1/3 同步项目版本号"
Sync-ProjectVersion $info.Version
Write-Host "  已写入 Cargo.toml / tauri.conf.json / package.json -> $($info.Version)"

if (-not $SkipBuild) {
    Write-Host ""
    Write-Host "2/3 打包安装包（npm run tauri build）"
    Push-Location $root
    try {
        npm run tauri build
        if ($LASTEXITCODE -ne 0) {
            throw "打包失败，退出码 $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
} else {
    Write-Host ""
    Write-Host "2/3 已跳过打包"
}

$installer = Find-Installer $info.Version
if ($installer) {
    Write-Host "  安装包：$($installer.FullName)"
} elseif ($SkipPublish) {
    Write-Host "  尚未生成安装包，已跳过查找"
} else {
    throw "未找到版本 $($info.Version) 的安装包，请确认打包成功：src-tauri\target\release\bundle\nsis\"
}

if (-not $SkipPublish) {
    Write-Host ""
    Write-Host "3/3 发布到更新服务器"
    Publish-Installer $installer.FullName $info.Version $info.Notes
} else {
    Write-Host ""
    Write-Host "3/3 已跳过发布"
}

Write-Host ""
Write-Host "完成。用户将看到版本 $($info.Version) 和 update.txt 中的更新日志。"
