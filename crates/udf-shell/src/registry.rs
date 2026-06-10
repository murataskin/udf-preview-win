//! Per-user (`HKCU\Software\Classes`) registration of the shell handlers — no admin needed.

use windows::core::{Result, GUID, PCWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, MAX_PATH};
use windows::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
    GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

use crate::com::{PREVIEW_CLSID, THUMBNAIL_CLSID};

/// Shell category GUID for `IThumbnailProvider` (`.ext\ShellEx\{...}`).
const CAT_THUMBNAIL: &str = "{e357fccd-a995-4576-b01f-234630154e96}";
/// Shell category GUID for `IPreviewHandler`.
const CAT_PREVIEW: &str = "{8895b1c6-b41f-4c1c-a562-0d564250836f}";
/// The **64-bit** prevhost surrogate AppID (`System32\prevhost.exe`) that runs preview
/// handlers out-of-process.
#[cfg(target_arch = "x86_64")]
const PREVHOST_APPID: &str = "{6d2b5079-2f0b-48dd-ab7f-97cec514d30b}";

/// The **32-bit** prevhost surrogate AppID (`SysWOW64\prevhost.exe`).
/// Required for 32-bit Outlook to load the previewer.
#[cfg(target_arch = "x86")]
const PREVHOST_APPID: &str = "{534A1E02-2BFE-4DF0-945D-255D5CE79298}";

fn guid_str(g: &GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7],
    )
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Create `<root>\<subkey>` and set a string value (default value if `name` None).
fn set_reg_string(root: HKEY, subkey: &str, name: Option<&str>, data: &str) -> Result<()> {
    unsafe {
        let mut hkey = HKEY::default();
        let err = RegCreateKeyExW(
            root,
            PCWSTR(wide(subkey).as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        );
        if err != ERROR_SUCCESS {
            return Err(windows::core::Error::from(err.to_hresult()));
        }
        let data_w = wide(data);
        let bytes = std::slice::from_raw_parts(
            data_w.as_ptr() as *const u8,
            std::mem::size_of_val(&data_w[..]),
        );
        let name_w = name.map(wide);
        let name_ptr = name_w
            .as_ref()
            .map(|v| PCWSTR(v.as_ptr()))
            .unwrap_or(PCWSTR::null());
        let err = RegSetValueExW(hkey, name_ptr, 0, REG_SZ, Some(bytes));
        let _ = RegCloseKey(hkey);
        if err != ERROR_SUCCESS {
            return Err(windows::core::Error::from(err.to_hresult()));
        }
        Ok(())
    }
}

fn delete_reg_tree(root: HKEY, subkey: &str) {
    unsafe {
        let _ = RegDeleteTreeW(root, PCWSTR(wide(subkey).as_ptr()));
    }
}

/// Absolute path of this DLL on disk.
fn module_path() -> Result<String> {
    unsafe {
        let mut hmodule = windows::Win32::Foundation::HMODULE::default();
        // Use GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS to find the handle of the DLL we are in.
        // We pass the address of this function itself.
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(module_path as *const u16),
            &mut hmodule,
        )?;
        let mut buf = [0u16; MAX_PATH as usize];
        let n = GetModuleFileNameW(hmodule, &mut buf);
        if n == 0 {
            return Err(E_FAIL.into());
        }
        Ok(String::from_utf16_lossy(&buf[..n as usize]))
    }
}

/// Register both handlers. 
/// 
/// CRITICAL: On 64-bit Windows, HKLM\Software\Classes is redirected for 32-bit apps to 
/// WOW6432Node. HKCU\Software\Classes is NOT redirected.
/// To support 32-bit Outlook and 64-bit Explorer simultaneously, we MUST use HKLM 
/// (requires Admin) or use architecture-specific CLSIDs (complex).
pub fn register() -> Result<()> {
    let dll = module_path()?;
    let thumb = guid_str(&THUMBNAIL_CLSID);
    let preview = guid_str(&PREVIEW_CLSID);

    // 1. Try HKLM (preferred for WOW6432Node support).
    let hklm = windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let hklm_res = (|| -> Result<()> {
        // Thumbnail CLSID
        set_reg_string(hklm, &format!("Software\\Classes\\CLSID\\{thumb}"), None, "UDF Thumbnail Handler")?;
        set_reg_string(hklm, &format!("Software\\Classes\\CLSID\\{thumb}\\InprocServer32"), None, &dll)?;
        set_reg_string(hklm, &format!("Software\\Classes\\CLSID\\{thumb}\\InprocServer32"), Some("ThreadingModel"), "Apartment")?;
        
        // Preview CLSID
        set_reg_string(hklm, &format!("Software\\Classes\\CLSID\\{preview}"), None, "UDF Preview Handler")?;
        set_reg_string(hklm, &format!("Software\\Classes\\CLSID\\{preview}"), Some("AppID"), PREVHOST_APPID)?;
        set_reg_string(hklm, &format!("Software\\Classes\\CLSID\\{preview}\\InprocServer32"), None, &dll)?;
        set_reg_string(hklm, &format!("Software\\Classes\\CLSID\\{preview}\\InprocServer32"), Some("ThreadingModel"), "Apartment")?;

        // Shell extension associations
        set_reg_string(hklm, &format!("Software\\Classes\\.udf\\ShellEx\\{CAT_THUMBNAIL}"), None, &thumb)?;
        set_reg_string(hklm, &format!("Software\\Classes\\.udf\\ShellEx\\{CAT_PREVIEW}"), None, &preview)?;
        
        // Approved handlers list
        set_reg_string(hklm, "Software\\Microsoft\\Windows\\CurrentVersion\\PreviewHandlers", Some(&preview), "UDF Preview Handler")?;
        Ok(())
    })();

    // 2. Fallback to HKCU if HKLM failed (e.g. no Admin).
    if hklm_res.is_err() {
        let hkcu = HKEY_CURRENT_USER;
        set_reg_string(hkcu, &format!("Software\\Classes\\CLSID\\{thumb}"), None, "UDF Thumbnail Handler")?;
        set_reg_string(hkcu, &format!("Software\\Classes\\CLSID\\{thumb}\\InprocServer32"), None, &dll)?;
        set_reg_string(hkcu, &format!("Software\\Classes\\CLSID\\{thumb}\\InprocServer32"), Some("ThreadingModel"), "Apartment")?;

        set_reg_string(hkcu, &format!("Software\\Classes\\CLSID\\{preview}"), None, "UDF Preview Handler")?;
        set_reg_string(hkcu, &format!("Software\\Classes\\CLSID\\{preview}"), Some("AppID"), PREVHOST_APPID)?;
        set_reg_string(hkcu, &format!("Software\\Classes\\CLSID\\{preview}\\InprocServer32"), None, &dll)?;
        set_reg_string(hkcu, &format!("Software\\Classes\\CLSID\\{preview}\\InprocServer32"), Some("ThreadingModel"), "Apartment")?;

        set_reg_string(hkcu, &format!("Software\\Classes\\.udf\\ShellEx\\{CAT_THUMBNAIL}"), None, &thumb)?;
        set_reg_string(hkcu, &format!("Software\\Classes\\.udf\\ShellEx\\{CAT_PREVIEW}"), None, &preview)?;
        
        set_reg_string(hkcu, "Software\\Microsoft\\Windows\\CurrentVersion\\PreviewHandlers", Some(&preview), "UDF Preview Handler")?;
    }

    Ok(())
}

/// Remove everything `register` created.
pub fn unregister() -> Result<()> {
    let thumb = guid_str(&THUMBNAIL_CLSID);
    let preview = guid_str(&PREVIEW_CLSID);
    
    let roots = [windows::Win32::System::Registry::HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER];
    for &root in &roots {
        delete_reg_tree(root, &format!("Software\\Classes\\CLSID\\{thumb}"));
        delete_reg_tree(root, &format!("Software\\Classes\\CLSID\\{preview}"));
        delete_reg_tree(root, &format!("Software\\Classes\\.udf\\ShellEx\\{CAT_THUMBNAIL}"));
        delete_reg_tree(root, &format!("Software\\Classes\\.udf\\ShellEx\\{CAT_PREVIEW}"));
        
        unsafe {
            let mut hkey = HKEY::default();
            if RegCreateKeyExW(
                root,
                PCWSTR(wide("Software\\Microsoft\\Windows\\CurrentVersion\\PreviewHandlers").as_ptr()),
                0,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut hkey,
                None,
            ) == ERROR_SUCCESS
            {
                let name_w = wide(&preview);
                let _ = windows::Win32::System::Registry::RegDeleteValueW(hkey, PCWSTR(name_w.as_ptr()));
                let _ = RegCloseKey(hkey);
            }
        }
    }
    Ok(())
}
