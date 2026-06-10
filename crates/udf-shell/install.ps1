# udf-preview-win — one-line installer for end users (no build, no toolchain).
#
#   irm https://github.com/saidsurucu/udf-preview-win/releases/latest/download/install.ps1 | iex
#
# Downloads the prebuilt, self-contained DLL, registers the .udf thumbnail handler (per-user,
# no admin) and the Preview Pane handler (one UAC prompt for the machine-wide approved list).
# Requires the Edge WebView2 Runtime (built into Windows 11; auto-installed here on Windows 10).

$ErrorActionPreference = 'Stop'
$repo = 'saidsurucu/udf-preview-win'
$dllUrl = "https://github.com/$repo/releases/latest/download/udf_shell.dll"
$previewClsid = '{7F3D9A21-4C8B-4E1A-9F2D-1A2B3C4D5E62}'

Write-Host 'UDF Önizleme kuruluyor...' -ForegroundColor Cyan

# 1) Ensure the Edge WebView2 Runtime (needed by the preview pane + viewer).
function Test-WebView2 {
    $keys = @(
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    )
    foreach ($k in $keys) { if ((Get-ItemProperty $k -ErrorAction SilentlyContinue).pv) { return $true } }
    return $false
}
if (-not (Test-WebView2)) {
    Write-Host 'WebView2 Runtime yok — kuruluyor...' -ForegroundColor Yellow
    $boot = "$env:TEMP\MicrosoftEdgeWebview2Setup.exe"
    Invoke-WebRequest 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $boot -UseBasicParsing
    Start-Process $boot -ArgumentList '/silent','/install' -Wait
}

# 2) Download self-contained DLLs to a stable per-user location.
$dir = "$env:LOCALAPPDATA\udf-preview"
New-Item -ItemType Directory -Force $dir | Out-Null
$dll = "$dir\udf_shell.dll"
$dllX86 = "$dir\udf_shell_x86.dll"

# Free previously-loaded copies so the download can overwrite them.
Stop-Process -Name explorer, dllhost, prevhost, msedgewebview2 -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 800

Invoke-WebRequest $dllUrl -OutFile $dll -UseBasicParsing
Invoke-WebRequest $dllX86Url -OutFile $dllX86 -UseBasicParsing
Unblock-File $dll
Unblock-File $dllX86

# 3) Register COM servers + .udf thumbnail/preview keys (per-user, HKCU).
& "$env:windir\System32\regsvr32.exe" /s $dll
if (Test-Path "$env:windir\SysWOW64\regsvr32.exe") {
    & "$env:windir\SysWOW64\regsvr32.exe" /s $dllX86
}
Write-Host 'Thumbnail kaydedildi (x64 + x86).' -ForegroundColor Green

# 4) Preview Pane needs the CLSID in the machine-wide HKLM approved list (one UAC prompt).
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()
          ).IsInRole([Security.Principal.WindowsBuiltinRole]::Administrator)

$hklmCmd64 = "reg add ""HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\PreviewHandlers"" /v ""$previewClsid"" /t REG_SZ /d ""UDF Preview Handler"" /f"
$hklmCmd32 = "reg add ""HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\PreviewHandlers"" /v ""$previewClsid"" /t REG_SZ /d ""UDF Preview Handler"" /f"

try {
    if ($isAdmin) {
        cmd /c $hklmCmd64 | Out-Null
        cmd /c $hklmCmd32 | Out-Null
    } else {
        Start-Process cmd.exe -ArgumentList "/c $hklmCmd64 & $hklmCmd32" -Verb RunAs -Wait
    }
    Write-Host 'Önizleme bölmesi ve Outlook entegrasyonu etkin.' -ForegroundColor Green
} catch {
    Write-Warning 'Önizleme bölmesi atlandı (yönetici onayı verilmedi).'
}

# 5) Pick up the new handler.
Get-ChildItem "$env:LOCALAPPDATA\Microsoft\Windows\Explorer" -Filter 'thumbcache_*.db' -ErrorAction SilentlyContinue |
    Remove-Item -Force -ErrorAction SilentlyContinue
if (-not (Get-Process explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }

Write-Host 'Kuruldu! .udf dosyaları artık küçük resim ve Önizleme Bölmesi (Alt+P) gösterir.' -ForegroundColor Cyan
Write-Host 'Kaldırmak için: irm https://github.com/saidsurucu/udf-preview-win/releases/latest/download/uninstall.ps1 | iex'
