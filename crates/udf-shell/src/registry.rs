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

/// Create `<root>\Software\Classes\<subkey>` and set a string value (default value if `name` None).
fn set_string(root: HKEY, subkey: &str, name: Option<&str>, data: &str) -> Result<()> {
    let full = format!("Software\\Classes\\{subkey}");
    set_string_in(root, &full, name, data)
}

fn set_string_in(root: HKEY, subkey: &str, name: Option<&str>, data: &str) -> Result<()> {
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

fn delete_tree(root: HKEY, subkey: &str) {
    let full = format!("Software\\Classes\\{subkey}");
    unsafe {
        let _ = RegDeleteTreeW(root, PCWSTR(wide(&full).as_ptr()));
    }
}

/// Absolute path of this DLL on disk.
fn module_path() -> Result<String> {
    unsafe {
        let mut hmodule = windows::Win32::Foundation::HMODULE::default();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(module_path as *const () as *const u16),
            &mut hmodule,
        )?;
        let mut buf = [0u16; MAX_PATH as usize];
        let n = GetModuleFileNameW(hmodule, &mut buf);
        Ok(String::from_utf16_lossy(&buf[..n as usize]))
    }
}

/// Register both handlers. We prioritize HKEY_LOCAL_MACHINE because it supports
/// architecture redirection (WOW6432Node), allowing 32-bit Outlook and 64-bit
/// Explorer to find their respective DLLs using the same CLSID.
pub fn register() -> Result<()> {
    let dll = module_path()?;
    let thumb = guid_str(&THUMBNAIL_CLSID);
    let preview = guid_str(&PREVIEW_CLSID);

    // Try HKLM first (requires admin), then fallback to HKCU.
    // HKCU does not support WOW6432Node redirection for Classes, so Outlook 32-bit
    // will likely fail if only registered in HKCU.
    let root = match set_string(windows::Win32::System::Registry::HKEY_LOCAL_MACHINE, &format!("CLSID\\{thumb}"), None, "UDF Thumbnail Handler") {
        Ok(_) => windows::Win32::System::Registry::HKEY_LOCAL_MACHINE,
        Err(_) => HKEY_CURRENT_USER,
    };

    // Thumbnail provider COM server.
    set_string(root, &format!("CLSID\\{thumb}"), None, "UDF Thumbnail Handler")?;
    set_string(root, &format!("CLSID\\{thumb}\\InprocServer32"), None, &dll)?;
    set_string(
        root,
        &format!("CLSID\\{thumb}\\InprocServer32"),
        Some("ThreadingModel"),
        "Apartment",
    )?;
    // Associate it with the .udf thumbnail slot.
    set_string(root, &format!(".udf\\ShellEx\\{CAT_THUMBNAIL}"), None, &thumb)?;

    // Preview handler COM server (runs in the prevhost surrogate).
    set_string(root, &format!("CLSID\\{preview}"), None, "UDF Preview Handler")?;
    set_string(root, &format!("CLSID\\{preview}"), Some("AppID"), PREVHOST_APPID)?;
    set_string(root, &format!("CLSID\\{preview}\\InprocServer32"), None, &dll)?;
    set_string(
        root,
        &format!("CLSID\\{preview}\\InprocServer32"),
        Some("ThreadingModel"),
        "Apartment",
    )?;
    set_string(root, &format!(".udf\\ShellEx\\{CAT_PREVIEW}"), None, &preview)?;

    // Approved preview handlers list (Machine-wide and User-wide).
    let _ = set_string_in(
        windows::Win32::System::Registry::HKEY_LOCAL_MACHINE,
        "Software\\Microsoft\\Windows\\CurrentVersion\\PreviewHandlers",
        Some(&preview),
        "UDF Preview Handler",
    );
    let _ = set_string_in(
        HKEY_CURRENT_USER,
        "Software\\Microsoft\\Windows\\CurrentVersion\\PreviewHandlers",
        Some(&preview),
        "UDF Preview Handler",
    );

    Ok(())
}

/// Remove everything `register` created.
pub fn unregister() -> Result<()> {
    let thumb = guid_str(&THUMBNAIL_CLSID);
    let preview = guid_str(&PREVIEW_CLSID);
    
    for &root in &[windows::Win32::System::Registry::HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        delete_tree(root, &format!("CLSID\\{thumb}"));
        delete_tree(root, &format!(".udf\\ShellEx\\{CAT_THUMBNAIL}"));
        delete_tree(root, &format!("CLSID\\{preview}"));
        delete_tree(root, &format!(".udf\\ShellEx\\{CAT_PREVIEW}"));
        
        unsafe {
            let name = wide(&preview);
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
                let _ = windows::Win32::System::Registry::RegDeleteValueW(hkey, PCWSTR(name.as_ptr()));
                let _ = RegCloseKey(hkey);
            }
        }
    }
    Ok(())
}
