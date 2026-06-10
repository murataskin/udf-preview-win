# udf-preview-win — one-line installer for end users (no build, no toolchain).
#
#   irm https://github.com/murataskin/udf-preview-win/releases/latest/download/install.ps1 | iex
#
# Downloads the prebuilt, self-contained DLLs and performs manual registry injection
# for both 64-bit and 32-bit environments. Requires Administrator privileges for HKLM.

$ErrorActionPreference = 'Stop'
$repo = 'murataskin/udf-preview-win'
$dllUrl = "https://github.com/$repo/releases/latest/download/udf_shell.dll"
$dllX86Url = "https://github.com/$repo/releases/latest/download/udf_shell_x86.dll"

$thumbClsid = '{7F3D9A21-4C8B-4E1A-9F2D-1A2B3C4D5E61}'
$previewClsid = '{7F3D9A21-4C8B-4E1A-9F2D-1A2B3C4D5E62}'
$prevhost64 = '{6d2b5079-2f0b-48dd-ab7f-97cec514d30b}'
$prevhost86 = '{534A1E02-2BFE-4DF0-945D-255D5CE79298}'

Write-Host 'UDF Önizleme kuruluyor (x64 + x86 Outlook desteği)...' -ForegroundColor Cyan

# 0) Check Administrator privileges
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltinRole]::Administrator)
if (-not $isAdmin) {
    Write-Error "Lütfen bu scripti YÖNETİCİ OLARAK (Run as Administrator) çalıştırın. Outlook kaydı için bu gereklidir."
    return
}

# 1) Ensure the Edge WebView2 Runtime
function Test-WebView2 {
    $keys = @(
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
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

# 2) Download DLLs
$dir = "$env:LOCALAPPDATA\udf-preview"
New-Item -ItemType Directory -Force $dir | Out-Null
$dll = "$dir\udf_shell.dll"
$dllX86 = "$dir\udf_shell_x86.dll"

Stop-Process -Name explorer, dllhost, prevhost, msedgewebview2 -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 800

Invoke-WebRequest $dllUrl -OutFile $dll -UseBasicParsing
Invoke-WebRequest $dllX86Url -OutFile $dllX86 -UseBasicParsing
Unblock-File $dll
Unblock-File $dllX86

# 3) Manual Registry Injection (The "regsvr32" bypass)
function Set-RegValue($path, $name, $value) {
    if (-not (Test-Path $path)) { New-Item -Path $path -Force | Out-Null }
    if ($name -eq "(default)") {
        Set-Item -Path $path -Value $value | Out-Null
    } else {
        Set-ItemProperty -Path $path -Name $name -Value $value -Type String -Force | Out-Null
    }
}

# --- 1. CLSID Registration (64-bit) ---
Write-Host 'x64 (Windows Gezgini) kaydı yapılıyor...' -ForegroundColor Gray
$clsid64 = "HKLM:\SOFTWARE\Classes\CLSID"
Set-RegValue "$clsid64\$thumbClsid" "(default)" "UDF Thumbnail Handler"
Set-RegValue "$clsid64\$thumbClsid\InprocServer32" "(default)" $dll
Set-RegValue "$clsid64\$thumbClsid\InprocServer32" "ThreadingModel" "Apartment"

Set-RegValue "$clsid64\$previewClsid" "(default)" "UDF Preview Handler"
Set-RegValue "$clsid64\$previewClsid" "AppID" $prevhost64
Set-RegValue "$clsid64\$previewClsid\InprocServer32" "(default)" $dll
Set-RegValue "$clsid64\$previewClsid\InprocServer32" "ThreadingModel" "Apartment"

# --- 2. CLSID Registration (32-bit for Outlook) ---
Write-Host 'x86 (Outlook 32-bit) kaydı yapılıyor...' -ForegroundColor Gray
$clsid32 = "HKLM:\SOFTWARE\Classes\WOW6432Node\CLSID"
Set-RegValue "$clsid32\$thumbClsid" "(default)" "UDF Thumbnail Handler"
Set-RegValue "$clsid32\$thumbClsid\InprocServer32" "(default)" $dllX86
Set-RegValue "$clsid32\$thumbClsid\InprocServer32" "ThreadingModel" "Apartment"

Set-RegValue "$clsid32\$previewClsid" "(default)" "UDF Preview Handler"
Set-RegValue "$clsid32\$previewClsid" "AppID" $prevhost86
Set-RegValue "$clsid32\$previewClsid\InprocServer32" "(default)" $dllX86
Set-RegValue "$clsid32\$previewClsid\InprocServer32" "ThreadingModel" "Apartment"

# --- 3. ProgID and File Association ---
Write-Host 'Dosya ilişkilendirmesi yapılıyor...' -ForegroundColor Gray
$progId = "udf_file"
Set-RegValue "HKLM:\SOFTWARE\Classes\$progId" "(default)" "UDF Belgesi"
Set-RegValue "HKLM:\SOFTWARE\Classes\$progId\ShellEx\{e357fccd-a995-4576-b01f-234630154e96}" "(default)" $thumbClsid
Set-RegValue "HKLM:\SOFTWARE\Classes\$progId\ShellEx\{8895b1c6-b41f-4c1c-a562-0d564250836f}" "(default)" $previewClsid

# Associate .udf with the ProgID
Set-RegValue "HKLM:\SOFTWARE\Classes\.udf" "(default)" $progId
Set-RegValue "HKLM:\SOFTWARE\Classes\.udf\ShellEx\{e357fccd-a995-4576-b01f-234630154e96}" "(default)" $thumbClsid
Set-RegValue "HKLM:\SOFTWARE\Classes\.udf\ShellEx\{8895b1c6-b41f-4c1c-a562-0d564250836f}" "(default)" $previewClsid

# --- 4. Approved Handlers List ---
Set-RegValue "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\PreviewHandlers" $previewClsid "UDF Preview Handler"
Set-RegValue "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\PreviewHandlers" $previewClsid "UDF Preview Handler"

# 4) Refresh Shell
Write-Host 'Önbellek temizleniyor...' -ForegroundColor Gray
Get-ChildItem "$env:LOCALAPPDATA\Microsoft\Windows\Explorer" -Filter 'thumbcache_*.db' -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
if (-not (Get-Process explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }

Write-Host 'Kuruldu! Outlook ve Gezgin artık UDF dosyalarını önizleyebilir.' -ForegroundColor Green
