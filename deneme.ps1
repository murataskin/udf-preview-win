$logFile = "$env:USERPROFILE\Desktop\UDF_Tehis_Log.txt"
$previewClsid = "{7F3D9A21-4C8B-4E1A-9F2D-1A2B3C4D5E62}"
$thumbClsid = "{7F3D9A21-4C8B-4E1A-9F2D-1A2B3C4D5E61}"

function Log($msg) {
    Write-Host $msg -ForegroundColor Cyan
    $msg | Out-File -FilePath $logFile -Append
}

# Eski logu temizle
if (Test-Path $logFile) { Remove-Item $logFile }

Log "=== UDF ÖNİZLEME TEŞHİS RAPORU ==="
Log "Tarih: $(Get-Date)"
Log "OS: $((Get-Caption) -join ' ')"
Log "Arch: $env:PROCESSOR_ARCHITECTURE"

Log "`n--- 1. Dosya Kontrolü ---"
$dir = "$env:LOCALAPPDATA\udf-preview"
if (Test-Path $dir) {
    Get-ChildItem $dir\udf_shell*.dll | ForEach-Object {
        Log "Dosya: $($_.Name) | Boyut: $($_.Length) | Yol: $($_.FullName)"
    }
} else {
    Log "HATA: Kurulum dizini bulunamadı: $dir"
}

Log "`n--- 2. Kayıt Defteri: Preview Handlers (HKLM) ---"
Log "64-bit HKLM:"
reg query "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\PreviewHandlers" /v $previewClsid 2>&1 | Out-File -FilePath $logFile -Append
Log "32-bit (WOW6432Node) HKLM:"
reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\PreviewHandlers" /v $previewClsid 2>&1 | Out-File -FilePath $logFile -Append

Log "`n--- 3. Kayıt Defteri: CLSID Kayıtları ---"
Log "Preview CLSID (x64):"
reg query "HKCR\CLSID\$previewClsid\InprocServer32" /ve 2>&1 | Out-File -FilePath $logFile -Append
Log "Preview CLSID (x86):"
reg query "HKLS\Software\Classes\WOW6432Node\CLSID\$previewClsid\InprocServer32" /ve 2>&1 | Out-File -FilePath $logFile -Append

Log "`n--- 4. Kayıt Defteri: .udf Uzantı Bağlantısı ---"
reg query "HKCU\Software\Classes\.udf\ShellEx\{8895b1c6-b41f-4c1c-a562-0d564250836f}" /ve 2>&1 | Out-File -FilePath $logFile -Append

Log "`n--- 5. Outlook Versiyonu ve WebView2 Kontrolü ---"
reg query "HKLM\SOFTWARE\Microsoft\Office\ClickToRun\Configuration" /v "Platform" 2>&1 | Out-File -FilePath $logFile -Append
reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v "pv" 2>&1 | Out-File -FilePath $logFile -Append

Log "`n--- 6. Manuel Kayıt Denemesi (Sadece Log için) ---"
if (Test-Path "$dir\udf_shell_x86.dll") {
    $proc = Start-Process regsvr32.exe -ArgumentList "/s", "`"$dir\udf_shell_x86.dll`"" -PassThru -Wait
    Log "regsvr32 x86 çıkış kodu: $($proc.ExitCode)"
}

Log "`n=== Teşhis Tamamlandı. Masaüstündeki UDF_Tehis_Log.txt dosyasını paylaşın. ==="


