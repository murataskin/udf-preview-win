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

# 2) Download the self-contained DLL to a stable per-user location.
$dir = "$env:LOCALAPPDATA\udf-preview"
New-Item -ItemType Directory -Force $dir | Out-Null
$dll = "$dir\udf_shell.dll"
# Free a previously-loaded copy so the download can overwrite it.
Stop-Process -Name explorer, dllhost, prevhost, msedgewebview2 -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 800
Invoke-WebRequest $dllUrl -OutFile $dll -UseBasicParsing
Unblock-File $dll   # strip Mark-of-the-Web so the shell loads it without a prompt

# 3) Register the COM server + .udf thumbnail/preview keys (per-user, HKCU).
& regsvr32.exe /s $dll
Write-Host 'Thumbnail kaydedildi (yönetici gerekmez).' -ForegroundColor Green

# 4) Preview Pane needs the CLSID in the machine-wide HKLM approved list (one UAC prompt).
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()
          ).IsInRole([Security.Principal.WindowsBuiltinRole]::Administrator)
$hklmCmd = "reg add ""HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\PreviewHandlers"" /v ""$previewClsid"" /t REG_SZ /d ""UDF Preview Handler"" /f"
try {
    if ($isAdmin) {
        cmd /c $hklmCmd | Out-Null
    } else {
        Start-Process cmd.exe -ArgumentList "/c $hklmCmd" -Verb RunAs -Wait
    }
    Write-Host 'Önizleme bölmesi etkin.' -ForegroundColor Green
} catch {
    Write-Warning 'Önizleme bölmesi atlandı (yönetici onayı verilmedi). Thumbnail yine de çalışır.'
}

# 5) Pick up the new handler.
Get-ChildItem "$env:LOCALAPPDATA\Microsoft\Windows\Explorer" -Filter 'thumbcache_*.db' -ErrorAction SilentlyContinue |
    Remove-Item -Force -ErrorAction SilentlyContinue
if (-not (Get-Process explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }

Write-Host 'Kuruldu! .udf dosyaları artık küçük resim ve Önizleme Bölmesi (Alt+P) gösterir.' -ForegroundColor Cyan
Write-Host 'Kaldırmak için: irm https://github.com/saidsurucu/udf-preview-win/releases/latest/download/uninstall.ps1 | iex'
