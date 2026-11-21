param(
    [switch]$Force
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$root = Split-Path -Parent $PSScriptRoot
$secretDir = Join-Path $PSScriptRoot ".signing"
$pfxPath = Join-Path $secretDir "rebased-codesign.pfx"
$passPath = Join-Path $secretDir "password.txt"
$publicDir = Join-Path $root "src-tauri\signing"
$cerPath = Join-Path $publicDir "rebased-codesign.cer"
$thumbPath = Join-Path $publicDir "thumbprint.txt"
$utf8 = New-Object System.Text.UTF8Encoding $false

if ((Test-Path $pfxPath) -and -not $Force) {
    Write-Host "签名证书已存在：$pfxPath"
    Write-Host "证书指纹：$((Get-Content $thumbPath -Raw).Trim())"
    Write-Host "如需重新生成请加 -Force（重新生成后，旧版本客户端将无法校验新安装包）"
    exit 0
}

New-Item -ItemType Directory -Force -Path $secretDir, $publicDir | Out-Null

$cert = New-SelfSignedCertificate `
    -Type CodeSigningCert `
    -Subject "CN=Rebased, O=Rebased" `
    -FriendlyName "Rebased Code Signing" `
    -KeyAlgorithm RSA `
    -KeyLength 3072 `
    -HashAlgorithm SHA256 `
    -KeyExportPolicy Exportable `
    -NotAfter (Get-Date).AddYears(20) `
    -CertStoreLocation "Cert:\CurrentUser\My"

$bytes = New-Object byte[] 24
[System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
$password = [Convert]::ToBase64String($bytes)
$secure = ConvertTo-SecureString -String $password -Force -AsPlainText

Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $secure | Out-Null
Export-Certificate -Cert $cert -FilePath $cerPath -Type CERT | Out-Null
[System.IO.File]::WriteAllText($passPath, $password, $utf8)
[System.IO.File]::WriteAllText($thumbPath, $cert.Thumbprint.ToUpperInvariant(), $utf8)

Remove-Item -Path "Cert:\CurrentUser\My\$($cert.Thumbprint)" -DeleteKey

Write-Host "已生成自签名代码签名证书"
Write-Host "  私钥（勿提交、务必备份）：$pfxPath"
Write-Host "  私钥密码（勿提交）：$passPath"
Write-Host "  公钥证书：$cerPath"
Write-Host "  证书指纹：$($cert.Thumbprint)"
