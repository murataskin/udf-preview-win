//! Phase 2 (Windows) — standalone viewer.
//!
//! A WebView2 window (hosted via `wry`/`tao`) showing `udf_to_html(file)`, with a small
//! toolbar carrying an **"UYAP Editör'de Aç"** button. The button posts an IPC message; the
//! Rust side `ShellExecute`s the `.udf` with its default association (the installed editor),
//! with a graceful message box when no editor / association is found.
//!
//! The rendered HTML is served over a custom `udf://` protocol (no NavigateToString size
//! limit, and a valid http URI for wry's IPC) rather than `with_html` or `file://`.

// GUI app: hide the console window in release builds. Keep it in debug so panics/logs show.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("udf-viewer is Windows-only.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    std::process::exit(windows_main());
}

#[cfg(windows)]
fn windows_main() -> i32 {
    use std::path::PathBuf;

    let Some(input) = std::env::args().nth(1) else {
        message_box("Kullanım: udf-viewer <dosya.udf>", "udf-viewer");
        return 2;
    };
    let input_path = PathBuf::from(&input);

    // Build the page: either the rendered document or an error page (never crash the GUI).
    let title = input_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "UDF".to_string());
    let page = match std::fs::read(&input_path) {
        Ok(bytes) => match udf_core::udf_to_html(&bytes) {
            Ok(html) => inject_toolbar(&html, &title),
            Err(e) => error_page(&format!("Belge ayrıştırılamadı: {e}")),
        },
        Err(e) => error_page(&format!("Dosya okunamadı: {input}\n{e}")),
    };

    // Serve the page over a custom `udf://` protocol (mapped to http://udf.localhost on
    // Windows). Two reasons not to use file:// or NavigateToString: NavigateToString caps at
    // ~2 MB, and a file:// page URL makes wry's IPC handler panic — it builds the IPC request
    // with the page URL as an `http::Uri`, and `file:///C:/...` fails to parse there.
    run_window(&title, page, input_path)
}

/// Events marshalled from the WebView IPC callback to the main event-loop thread. Doing the
/// `ShellExecute`/`MessageBox` work here (not inside the wry IPC handler) avoids reentrancy
/// into the WebView2 message pump, which crashes the process.
#[cfg(windows)]
enum UserEvent {
    OpenEditor,
}

#[cfg(windows)]
fn run_window(title: &str, page: String, udf_path: std::path::PathBuf) -> i32 {
    use std::borrow::Cow;

    use tao::{
        dpi::LogicalSize,
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoopBuilder},
        window::WindowBuilder,
    };
    use wry::{
        http::{header::CONTENT_TYPE, Request, Response},
        WebViewBuilder,
    };

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let window = match WindowBuilder::new()
        .with_title(format!("{title} — UDF Görüntüleyici"))
        .with_inner_size(LogicalSize::new(900.0, 1000.0))
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => {
            message_box(&format!("Pencere oluşturulamadı: {e}"), "udf-viewer");
            return 1;
        }
    };

    // The IPC handler only forwards a lightweight event; no Win32 modal work happens on the
    // WebView2 callback thread.
    let proxy = event_loop.create_proxy();
    let page_bytes = page.into_bytes();
    let webview = WebViewBuilder::new(&window)
        .with_custom_protocol("udf".to_string(), move |_req: Request<Vec<u8>>| {
            Response::builder()
                .header(CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Cow::Owned(page_bytes.clone()))
                .unwrap()
        })
        .with_url("udf://localhost/")
        .with_ipc_handler(move |req: Request<String>| {
            if req.body() == "open-editor" {
                let _ = proxy.send_event(UserEvent::OpenEditor);
            }
        })
        .build();

    let webview = match webview {
        Ok(wv) => wv,
        Err(e) => {
            message_box(
                &format!("WebView2 başlatılamadı (Edge WebView2 Runtime kurulu mu?):\n{e}"),
                "udf-viewer",
            );
            return 1;
        }
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        // Keep the window and webview alive for the whole loop.
        let _ = (&window, &webview);
        match event {
            Event::UserEvent(UserEvent::OpenEditor) => open_in_editor(&udf_path),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}

/// Open the `.udf` with its default association (the installed UYAP editor), via ShellExecute.
#[cfg(windows)]
fn open_in_editor(path: &std::path::Path) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let file = wide(&path.to_string_lossy());
    let verb = wide("open");
    // Returns an HINSTANCE; a value <= 32 indicates failure (e.g. no association).
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as usize <= 32 {
        message_box(
            "UYAP Editör açılamadı.\n\n.udf uzantısı bir uygulamayla ilişkilendirilmemiş \
             olabilir veya UYAP Doküman Editörü kurulu değil.",
            "UYAP Editör'de Aç",
        );
    }
}

#[cfg(windows)]
fn message_box(text: &str, caption: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};
    let t = wide(text);
    let c = wide(caption);
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(t.as_ptr()),
            PCWSTR(c.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Inject a fixed toolbar (with the Editör button) just after `<body>`, plus a spacer so it
/// doesn't cover the page. `udf_core`'s HTML always emits a bare `<body>` tag.
#[cfg(windows)]
fn inject_toolbar(html: &str, filename: &str) -> String {
    let toolbar = format!(
        "<div style=\"position:fixed;top:0;left:0;right:0;height:40px;background:#2b2b2b;\
display:flex;align-items:center;padding:0 12px;z-index:99999;\
box-shadow:0 1px 4px rgba(0,0,0,.35);font-family:Segoe UI,sans-serif;\">\
<button onclick=\"window.ipc.postMessage('open-editor')\" \
style=\"background:#0a6cff;color:#fff;border:0;border-radius:4px;padding:7px 14px;\
font-size:13px;cursor:pointer;\">UYAP Editör'de Aç</button>\
<span style=\"color:#cfcfcf;margin-left:12px;font-size:13px;\">{}</span>\
</div><div style=\"height:48px;\"></div>",
        html_escape(filename)
    );
    html.replacen("<body>", &format!("<body>{toolbar}"), 1)
}

#[cfg(windows)]
fn error_page(message: &str) -> String {
    format!(
        "<!doctype html><html lang=\"tr\"><head><meta charset=\"utf-8\"/></head>\
<body style=\"font-family:Segoe UI,sans-serif;padding:32px;color:#333;\">\
<h2>Görüntülenemedi</h2><pre style=\"white-space:pre-wrap;color:#a00;\">{}</pre></body></html>",
        html_escape(message)
    )
}

#[cfg(windows)]
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
