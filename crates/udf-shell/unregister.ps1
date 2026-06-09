# Unregister the udf-shell handlers. Run elevated to also remove the HKLM preview entry.
$ErrorActionPreference = 'SilentlyContinue'
$previewClsid = '{7F3D9A21-4C8B-4E1A-9F2D-1A2B3C4D5E62}'
$dll = Join-Path $env:LOCALAPPDATA 'udf-preview\udf_shell.dll'

if (Test-Path $dll) { & regsvr32.exe /s /u $dll }

$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()
          ).IsInRole([Security.Principal.WindowsBuiltinRole]::Administrator)
if ($isAdmin) {
    reg delete 'HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\PreviewHandlers' /v $previewClsid /f | Out-Null
}

Stop-Process -Name explorer -Force
Start-Sleep -Milliseconds 800
Remove-Item "$env:LOCALAPPDATA\Microsoft\Windows\Explorer\thumbcache_*.db" -Force
if (-not (Get-Process explorer)) { Start-Process explorer.exe }
Write-Host 'Unregistered.'
