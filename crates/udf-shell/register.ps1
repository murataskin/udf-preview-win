# Register the udf-shell handlers (thumbnail + preview) for the current user.
#
# Thumbnail registration is per-user (HKCU) and needs no admin. The Preview Pane additionally
# requires the handler's CLSID in the machine-wide approved list under HKLM, so that one step
# needs elevation — run this script from an elevated PowerShell to enable the preview pane.
#
#   pwsh -File register.ps1            # thumbnail only (per-user)
#   (elevated) pwsh -File register.ps1 # thumbnail + preview pane
#
# Build first:  cargo build -p udf-shell --release

$ErrorActionPreference = 'Stop'
$previewClsid = '{7F3D9A21-4C8B-4E1A-9F2D-1A2B3C4D5E62}'

$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$dllSrc = Join-Path $repo 'target\release\udf_shell.dll'
if (-not (Test-Path $dllSrc)) {
    throw "DLL not found: $dllSrc  (run: cargo build -p udf-shell --release)"
}

# Copy to a stable per-user location so rebuilds don't fight the loaded DLL.
$dstDir = Join-Path $env:LOCALAPPDATA 'udf-preview'
New-Item -ItemType Directory -Force $dstDir | Out-Null
$dll = Join-Path $dstDir 'udf_shell.dll'
Copy-Item $dllSrc $dll -Force

# DllRegisterServer writes the HKCU class/ShellEx keys (thumbnail + preview CLSID/InprocServer32).
& regsvr32.exe /s $dll
Write-Host "Registered (HKCU): $dll"

# Preview Pane approved-handlers list — HKLM, needs admin.
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()
          ).IsInRole([Security.Principal.WindowsBuiltinRole]::Administrator)
if ($isAdmin) {
    reg add 'HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\PreviewHandlers' `
        /v $previewClsid /t REG_SZ /d 'UDF Preview Handler' /f | Out-Null
    Write-Host 'Registered preview handler in HKLM approved list (Preview Pane enabled).'
} else {
    Write-Warning 'Not elevated: skipped HKLM preview approved-list entry. The Preview Pane will'
    Write-Warning 'not show .udf until you re-run this elevated. (Thumbnails work either way.)'
}

# Pick up the new handler: clear the thumbnail cache and restart Explorer.
Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 800
Remove-Item "$env:LOCALAPPDATA\Microsoft\Windows\Explorer\thumbcache_*.db" -Force -ErrorAction SilentlyContinue
if (-not (Get-Process explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }
Write-Host 'Done. Note: OneDrive placeholder files may not thumbnail until downloaded locally.'
