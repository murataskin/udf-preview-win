# udf-preview-win — one-line uninstaller for end users.
#
#   irm https://github.com/saidsurucu/udf-preview-win/releases/latest/download/uninstall.ps1 | iex
#
# Unregisters both handlers and removes the installed DLL. Self-elevates only to drop the
# machine-wide HKLM preview approved-list entry (one UAC prompt).

$ErrorActionPreference = 'SilentlyContinue'
$previewClsid = '{7F3D9A21-4C8B-4E1A-9F2D-1A2B3C4D5E62}'
$dll = "$env:LOCALAPPDATA\udf-preview\udf_shell.dll"

Stop-Process -Name explorer, dllhost, prevhost, msedgewebview2 -Force
Start-Sleep -Milliseconds 800

if (Test-Path $dll) { & regsvr32.exe /s /u $dll }   # removes the HKCU class/ShellEx keys

# HKLM preview approved-list entry (one UAC prompt).
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()
          ).IsInRole([Security.Principal.WindowsBuiltinRole]::Administrator)
$cmd = "reg delete ""HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\PreviewHandlers"" /v ""$previewClsid"" /f"
try {
    if ($isAdmin) { cmd /c $cmd | Out-Null } else { Start-Process cmd.exe -ArgumentList "/c $cmd" -Verb RunAs -Wait }
} catch {}

Remove-Item "$env:LOCALAPPDATA\udf-preview" -Recurse -Force
Remove-Item "$env:USERPROFILE\AppData\LocalLow\udf-preview" -Recurse -Force
Get-ChildItem "$env:LOCALAPPDATA\Microsoft\Windows\Explorer" -Filter 'thumbcache_*.db' | Remove-Item -Force
if (-not (Get-Process explorer)) { Start-Process explorer.exe }
Write-Host 'Kaldırıldı.' -ForegroundColor Cyan
