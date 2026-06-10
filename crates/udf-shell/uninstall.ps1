# udf-preview-win — one-line uninstaller for end users.
$ErrorActionPreference = 'SilentlyContinue'

$thumbClsid = '{7F3D9A21-4C8B-4E1A-9F2D-1A2B3C4D5E61}'
$previewClsid = '{7F3D9A21-4C8B-4E1A-9F2D-1A2B3C4D5E62}'
$dir = "$env:LOCALAPPDATA\udf-preview"

Write-Host 'UDF Önizleme kaldırılıyor...' -ForegroundColor Cyan

Stop-Process -Name explorer, dllhost, prevhost, msedgewebview2 -Force
Start-Sleep -Milliseconds 800

# Remove Registry Keys (Native and WOW64)
$keys = @(
    "HKLM:\SOFTWARE\Classes\CLSID\$thumbClsid",
    "HKLM:\SOFTWARE\Classes\CLSID\$previewClsid",
    "HKLM:\SOFTWARE\Classes\WOW6432Node\CLSID\$thumbClsid",
    "HKLM:\SOFTWARE\Classes\WOW6432Node\CLSID\$previewClsid",
    "HKLM:\SOFTWARE\Classes\.udf",
    "HKCU:\Software\Classes\CLSID\$thumbClsid",
    "HKCU:\Software\Classes\CLSID\$previewClsid",
    "HKCU:\Software\Classes\.udf"
)

foreach ($key in $keys) {
    if (Test-Path $key) { Remove-Item $key -Recurse -Force | Out-Null }
}

# Remove from Approved Handlers
$handlerKeys = @(
    "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\PreviewHandlers",
    "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\PreviewHandlers",
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\PreviewHandlers"
)
foreach ($hk in $handlerKeys) {
    if (Test-Path $hk) {
        $val = Get-ItemProperty $hk -Name $previewClsid -ErrorAction SilentlyContinue
        if ($val) { Remove-ItemProperty $hk -Name $previewClsid -Force | Out-Null }
    }
}

# Cleanup Files
Remove-Item $dir -Recurse -Force
Remove-Item "$env:USERPROFILE\AppData\LocalLow\udf-preview" -Recurse -Force

if (-not (Get-Process explorer)) { Start-Process explorer.exe }
Write-Host 'Kaldırıldı.' -ForegroundColor Green
