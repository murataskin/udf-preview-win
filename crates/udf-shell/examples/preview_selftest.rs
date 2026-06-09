//! Diagnostic: drive the *registered* preview handler through COM in a real window —
//! CoCreateInstance(PREVIEW_CLSID) -> Initialize(stream) -> SetWindow/DoPreview -> message
//! loop. This proves the preview handler renders end-to-end without depending on Explorer's
//! HKLM approved-handlers list (which is the only extra thing the Preview Pane needs).
//!
//!   cargo run -p udf-shell --example preview_selftest -- <file.udf>

#[cfg(not(windows))]
fn main() {
    eprintln!("preview_selftest is Windows-only.");
}

#[cfg(windows)]
fn main() {
    if let Err(e) = run() {
        eprintln!("FAILED: {e} (0x{:08X})", e.code().0);
        std::process::exit(1);
    }
}

#[cfg(windows)]
unsafe extern "system" fn wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wp: windows::Win32::Foundation::WPARAM,
    lp: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{DefWindowProcW, PostQuitMessage, WM_DESTROY};
    if msg == WM_DESTROY {
        PostQuitMessage(0);
        return windows::Win32::Foundation::LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

#[cfg(windows)]
fn run() -> windows::core::Result<()> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::PropertiesSystem::IInitializeWithStream;
    use windows::Win32::UI::Shell::{IPreviewHandler, SHCreateMemStream};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, GetClientRect, GetMessageW, RegisterClassW, ShowWindow,
        TranslateMessage, CW_USEDEFAULT, MSG, SW_SHOW, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };

    let path = std::env::args()
        .nth(1)
        .expect("usage: preview_selftest <file.udf>");
    let bytes = std::fs::read(&path).expect("read .udf");

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let hinst = GetModuleHandleW(None)?;

        let class_name = windows::core::w!("UdfPreviewSelftest");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            &HSTRING::from(format!("UDF Preview self-test — {path}")),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            900,
            1000,
            None,
            None,
            hinst,
            None,
        )?;
        let _ = ShowWindow(hwnd, SW_SHOW);

        // Drive the preview handler.
        let stream = SHCreateMemStream(Some(&bytes)).expect("memstream");
        let init: IInitializeWithStream =
            CoCreateInstance(&udf_shell::PREVIEW_CLSID, None, CLSCTX_INPROC_SERVER)?;
        println!("CoCreateInstance(preview) OK");
        init.Initialize(&stream, 0)?;
        let ph: IPreviewHandler = init.cast()?;
        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc)?;
        ph.SetWindow(hwnd, &rc)?;
        ph.DoPreview()?;
        println!("DoPreview OK — rendering; close the window to exit");

        // Pump messages so WebView2 renders and the window stays interactive.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = ph.Unload();
        Ok(())
    }
}
