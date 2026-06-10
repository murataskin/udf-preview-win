$ErrorActionPreference = 'SilentlyContinue'
$logPath = "$env:USERPROFILE\Desktop\udf_preview_diag.log"

Start-Transcript -Path $logPath -Force | Out-Null
Write-Output "=================================================="
Write-Output " UDF PREVIEW HANDLER DIAGNOSTIC SCRIPT"
Write-Output " Date: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
Write-Output "=================================================="

function Check-RegKey {
    param($Path, $Name = $null)
    $val = $null
    if ($Name) {
        $val = (Get-ItemProperty -Path $Path -Name $Name -ErrorAction SilentlyContinue).$Name
        if ($null -ne $val) { Write-Output "[OK] $Path -> $Name : $val" }
        else { Write-Output "[MISSING/EMPTY] $Path -> $Name" }
    } else {
        $key = Get-Item -Path $Path -ErrorAction SilentlyContinue
        if ($key) {
            Write-Output "[OK] KEY EXISTS: $Path"
            $key.Property | ForEach-Object {
                $v = $key.GetValue($_)
                Write-Output "      $_ = $v"
            }
        } else { Write-Output "[MISSING] KEY: $Path" }
    }
}

Write-Output "`n--- 1. SYSTEM & OS INFO ---"
Get-CimInstance Win32_OperatingSystem | Select-Object Caption, OSArchitecture, Version | Format-List

Write-Output "`n--- 2. OUTLOOK & OFFICE INFO ---"
Check-RegKey "HKLM:\SOFTWARE\Microsoft\Office\ClickToRun\Configuration" "Architecture"
Check-RegKey "HKLM:\SOFTWARE\Microsoft\Office\ClickToRun\Configuration" "VersionToReport"
Check-RegKey "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Office\ClickToRun\Configuration" "Architecture"
$outlookPath = (Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\OUTLOOK.EXE" -ErrorAction SilentlyContinue).'(default)'
Write-Output "Outlook.exe Path: $outlookPath"

Write-Output "`n--- 3. UDF EXTENSION REGISTRATION ---"
Check-RegKey "HKCR:\.udf" "(default)"
Check-RegKey "HKCR:\.udf\ShellEx\{8895b1c6-b41f-4c1c-a562-0d564250836f}" "(default)"
Check-RegKey "HKCU:\Software\Classes\.udf\ShellEx\{8895b1c6-b41f-4c1c-a562-0d564250836f}" "(default)"

Write-Output "`n--- 4. CLSID REGISTRATIONS (PREVIEW HANDLER) ---"
$clsid = "{7F3D9A21-4C8B-4E1A-9F2D-1A2B3C4D5E62}"
Write-Output ">> 64-bit / Standard CLSID (HKCU & HKCR)"
Check-RegKey "HKCU:\Software\Classes\CLSID\$clsid" "AppID"
Check-RegKey "HKCU:\Software\Classes\CLSID\$clsid\InprocServer32" "(default)"
Check-RegKey "HKCR:\CLSID\$clsid" "AppID"
Check-RegKey "HKCR:\CLSID\$clsid\InprocServer32" "(default)"

Write-Output ">> 32-bit / WOW6432Node CLSID (HKCU & HKCR)"
Check-RegKey "HKCU:\Software\Classes\WOW6432Node\CLSID\$clsid" "AppID"
Check-RegKey "HKCU:\Software\Classes\WOW6432Node\CLSID\$clsid\InprocServer32" "(default)"
Check-RegKey "HKCR:\WOW6432Node\CLSID\$clsid" "AppID"
Check-RegKey "HKCR:\WOW6432Node\CLSID\$clsid\InprocServer32" "(default)"

Write-Output "`n--- 5. APPROVED HANDLERS LIST ---"
Check-RegKey "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\PreviewHandlers" $clsid
Check-RegKey "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\PreviewHandlers" $clsid
Check-RegKey "HKCU:\Software\Microsoft\Windows\CurrentVersion\PreviewHandlers" $clsid

Write-Output "`n--- 6. DLL FILE VERIFICATION ---"
$dllDir = "$env:LOCALAPPDATA\udf-preview"
$dll64 = "$dllDir\udf_shell.dll"
$dll32 = "$dllDir\udf_shell_x86.dll"

foreach ($file in @($dll64, $dll32)) {
    if (Test-Path $file) {
        $size = (Get-Item $file).Length
        Write-Output "[OK] Found: $file ($size bytes)"
        $motw = Get-Item -Path $file -Stream "Zone.Identifier" -ErrorAction SilentlyContinue
        if ($motw) { Write-Output "     [WARNING] Mark-of-the-Web (Zone.Identifier) is present!" }
    } else {
        Write-Output "[MISSING] $file"
    }
}

Write-Output "`n--- 7. WEBVIEW2 REGISTRATION ---"
Check-RegKey "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"

Write-Output "`n--- 8. EVENT VIEWER CRASH/ERROR LOGS (Last 48 Hours) ---"
$events = Get-WinEvent -FilterHashtable @{LogName='Application'; StartTime=(Get-Date).AddDays(-2)} -ErrorAction SilentlyContinue | 
          Where-Object { $_.Message -match "(prevhost|udf_shell|WebView2|Outlook)" -and ($_.LevelDisplayName -eq "Error" -or $_.LevelDisplayName -eq "Warning") } |
          Select-Object TimeCreated, Id, LevelDisplayName, Message

if ($events) {
    $events | ForEach-Object {
        Write-Output "[$($_.TimeCreated)] [EventID: $($_.Id)] [$($_.LevelDisplayName)]"
        Write-Output "$($_.Message)"
        Write-Output "-----------------"
    }
} else {
    Write-Output "No relevant crashes or errors found in Application log."
}

Write-Output "`n=================================================="
Write-Output " DIAGNOSTIC COMPLETE"
Write-Output "=================================================="
Stop-Transcript | Out-Null
Write-Host "Log dosyasi masaustunuze kaydedildi: udf_preview_diag.log" -ForegroundColor Green