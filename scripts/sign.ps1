param(
    [Parameter(Mandatory = $true)]
    [string]$File
)

$ErrorActionPreference = "Stop"

$secretDir = Join-Path $PSScriptRoot ".signing"
$pfxPath = Join-Path $secretDir "rebased-codesign.pfx"
$passPath = Join-Path $secretDir "password.txt"

if (-not (Test-Path $pfxPath)) {
    throw "找不到签名证书 $pfxPath，请先运行 scripts\signing-cert.ps1"
}

function Find-SignTool {
    $cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
    $found = Get-ChildItem $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match "\\x64\\" } |
        Sort-Object FullName -Descending |
        Select-Object -First 1
    if ($found) { return $found.FullName }
    throw "找不到 signtool.exe，请安装 Windows 10/11 SDK"
}

$signtool = Find-SignTool
$password = (Get-Content $passPath -Raw).Trim()

$signArgs = @("sign", "/fd", "SHA256", "/f", $pfxPath, "/p", $password, "/d", "Rebased")
if ($env:REBASED_TIMESTAMP_URL) {
    $signArgs += @("/tr", $env:REBASED_TIMESTAMP_URL, "/td", "SHA256")
}
$signArgs += $File

& $signtool @signArgs | Out-Host
if ($LASTEXITCODE -ne 0) {
    throw "签名失败：$File（退出码 $LASTEXITCODE）"
}
