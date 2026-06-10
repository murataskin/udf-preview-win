# udf-preview-win — one-line uninstaller for end users.
#
#   irm https://github.com/murataskin/udf-preview-win/releases/latest/download/uninstall.ps1 | iex
#
# Unregisters both handlers and removes the installed DLL. Self-elevates only to drop the
# machine-wide HKLM preview approved-list entry (one UAC prompt).

$ErrorActionPreference = 'SilentlyContinue'
$previewClsid = '{7F3D9A21-4C8B-4E1A-9F2D-1A2B3C4D5E62}'
$dir = "$env:LOCALAPPDATA\udf-preview"
$dll = "$dir\udf_shell.dll"
$dllX86 = "$dir\udf_shell_x86.dll"

Stop-Process -Name explorer, dllhost, prevhost, msedgewebview2 -Force
Start-Sleep -Milliseconds 800

if (Test-Path $dll) { & "$env:windir\System32\regsvr32.exe" /s /u $dll }
if (Test-Path $dllX86) {
    if (Test-Path "$env:windir\SysWOW64\regsvr32.exe") {
        & "$env:windir\SysWOW64\regsvr32.exe" /s /u $dllX86
    }
}

# HKLM preview approved-list entries (one UAC prompt).
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()
          ).IsInRole([Security.Principal.WindowsBuiltinRole]::Administrator)

$cmd64 = "reg delete ""HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\PreviewHandlers"" /v ""$previewClsid"" /f"
$cmd32 = "reg delete ""HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\PreviewHandlers"" /v ""$previewClsid"" /f"

try {
    if ($isAdmin) {
        cmd /c $cmd64 | Out-Null
        cmd /c $cmd32 | Out-Null
    } else {
        Start-Process cmd.exe -ArgumentList "/c $cmd64 & $cmd32" -Verb RunAs -Wait
    }
} catch {}

Remove-Item $dir -Recurse -Force
Remove-Item "$env:USERPROFILE\AppData\LocalLow\udf-preview" -Recurse -Force
Get-ChildItem "$env:LOCALAPPDATA\Microsoft\Windows\Explorer" -Filter 'thumbcache_*.db' | Remove-Item -Force
if (-not (Get-Process explorer)) { Start-Process explorer.exe }
Write-Host 'Kaldırıldı.' -ForegroundColor Cyan
