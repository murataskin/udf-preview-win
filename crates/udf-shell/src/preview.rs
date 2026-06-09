//! Preview handler — renders `udf_to_html` in a WebView2 child window hosted in the Preview
//! Pane's parent HWND (provided by the host via `IPreviewHandler::SetWindow`).
//!
//! WebView2 is created with wry's `new_as_child` over a thin `HasWindowHandle` wrapper around
//! the raw HWND. Hard-won details that make this actually render in Explorer's pane:
//!   * the HTML is served via a custom `udf://` protocol (no NavigateToString size limit; a
//!     valid http origin), with a unique URL per load to defeat WebView2's response cache;
//!   * WebView2's user-data folder is pinned under `LocalLow`, the only place the **low
//!     integrity** preview surrogate (`prevhost.exe`) can write — otherwise the pane is blank;
//!   * bounds are passed as **physical** pixels (the host's rect is physical); passing them as
//!     logical makes wry re-scale by the monitor DPI and oversize/clip the webview;
//!   * the A4 page is scaled to the pane width via injected CSS `zoom` so it fits and scrolls
//!     vertically instead of being clipped.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::num::NonZeroIsize;

use windows::core::{implement, Error, IUnknown, Interface, Result, GUID};
use windows::Win32::Foundation::{E_FAIL, E_POINTER, HWND, RECT, S_FALSE};
use windows::Win32::System::Com::IStream;
use windows::Win32::System::Ole::{
    IObjectWithSite, IObjectWithSite_Impl, IOleWindow, IOleWindow_Impl,
};
use windows::Win32::UI::Shell::PropertiesSystem::{
    IInitializeWithStream, IInitializeWithStream_Impl,
};
use windows::Win32::UI::Shell::{IPreviewHandler, IPreviewHandler_Impl};
use windows::Win32::UI::WindowsAndMessaging::MSG;

use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::http::{header::CONTENT_TYPE, Request, Response};
use wry::raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use wry::{Rect, WebViewBuilder};

use crate::com::{guard_com, read_all_stream, DllGuard};

/// WebView2 user-data folder, under `LocalLow` so the **low-integrity** preview surrogate
/// (`prevhost.exe`) can write it — `%LOCALAPPDATA%` is medium-integrity and denied there,
/// which otherwise leaves the environment uninitialised and the pane blank.
fn webview_data_dir() -> std::path::PathBuf {
    let base = std::env::var_os("USERPROFILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    base.join("AppData")
        .join("LocalLow")
        .join("udf-preview")
        .join("webview2")
}

/// Scale the A4 page down to the (usually narrow) preview-pane width via CSS `zoom`, so the
/// document fits horizontally and scrolls vertically instead of being clipped. Injected only
/// for the preview pane — the core HTML and the standalone viewer keep the full A4 width.
fn fit_to_width(html: &str) -> String {
    // `overflow-y:scroll` reserves the vertical scrollbar up-front so `clientWidth` is stable
    // when we measure (otherwise the scrollbar appears after fitting and steals ~17px); the
    // body gutter gives a small margin; the page is left-aligned (not `margin:auto`, which
    // mis-centres under `zoom`).
    const INJECT: &str = "<style>html{overflow-x:hidden;overflow-y:scroll}\
body{margin:0!important;padding:14px!important;box-sizing:border-box;overflow-x:hidden}\
.udf-page{margin:0!important}</style>\
<script>(function(){function fit(){var p=document.querySelector('.udf-page');if(!p)return;\
p.style.zoom='1';\
var pw=Math.max(p.offsetWidth,p.scrollWidth,document.documentElement.scrollWidth);\
var avail=document.documentElement.clientWidth-36;\
if(pw>0){var s=avail/pw;if(s>1)s=1;p.style.zoom=s;}}\
window.addEventListener('load',fit);window.addEventListener('resize',fit);fit();})();</script>";
    html.replacen("</body>", &format!("{INJECT}</body>"), 1)
}

/// Monotonic id to make each preview load a unique URL (defeats WebView2's response cache).
fn next_load_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Minimal `HasWindowHandle` over a raw Win32 HWND so wry can attach a child WebView2.
struct HwndHost(isize);

impl HasWindowHandle for HwndHost {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let h = NonZeroIsize::new(self.0).ok_or(HandleError::Unavailable)?;
        let handle = RawWindowHandle::Win32(Win32WindowHandle::new(h));
        Ok(unsafe { WindowHandle::borrow_raw(handle) })
    }
}

#[implement(IInitializeWithStream, IPreviewHandler, IObjectWithSite, IOleWindow)]
#[derive(Default)]
pub struct UdfPreviewHandler {
    stream: RefCell<Option<IStream>>,
    site: RefCell<Option<IUnknown>>,
    parent: Cell<isize>,
    rect: RefCell<RECT>,
    web_context: RefCell<Option<wry::WebContext>>,
    webview: RefCell<Option<wry::WebView>>,
    _guard: DllGuard,
}

impl IInitializeWithStream_Impl for UdfPreviewHandler_Impl {
    fn Initialize(&self, pstream: Option<&IStream>, _grfmode: u32) -> Result<()> {
        *self.stream.borrow_mut() = pstream.cloned();
        Ok(())
    }
}

impl IPreviewHandler_Impl for UdfPreviewHandler_Impl {
    fn SetWindow(&self, hwnd: HWND, prc: *const RECT) -> Result<()> {
        self.parent.set(hwnd.0 as isize);
        if !prc.is_null() {
            *self.rect.borrow_mut() = unsafe { *prc };
        }
        self.apply_bounds();
        Ok(())
    }

    fn SetRect(&self, prc: *const RECT) -> Result<()> {
        if !prc.is_null() {
            *self.rect.borrow_mut() = unsafe { *prc };
        }
        self.apply_bounds();
        Ok(())
    }

    fn DoPreview(&self) -> Result<()> {
        guard_com(|| {
            let stream = self
                .stream
                .borrow()
                .clone()
                .ok_or_else(|| Error::from(E_FAIL))?;
            let bytes = unsafe { read_all_stream(&stream)? };
            let html =
                fit_to_width(&udf_core::udf_to_html(&bytes).map_err(|_| Error::from(E_FAIL))?);

            let parent = self.parent.get();
            if parent == 0 {
                return Err(Error::from(E_FAIL));
            }
            let host = HwndHost(parent);
            let html_bytes = html.into_bytes();

            let data_dir = webview_data_dir();
            let _ = std::fs::create_dir_all(&data_dir);
            let mut web_context = wry::WebContext::new(Some(data_dir));

            let webview = WebViewBuilder::new_as_child(&host)
                .with_web_context(&mut web_context)
                .with_bounds(self.bounds())
                .with_custom_protocol("udf".to_string(), move |_req: Request<Vec<u8>>| {
                    Response::builder()
                        .header(CONTENT_TYPE, "text/html; charset=utf-8")
                        .header("Cache-Control", "no-store")
                        .body(Cow::Owned(html_bytes.clone()))
                        .unwrap()
                })
                .with_url(format!("udf://localhost/?v={}", next_load_id()))
                .build()
                .map_err(|_| Error::from(E_FAIL))?;
            // The &mut borrow ended with build(); keep the context alive alongside the webview.
            *self.web_context.borrow_mut() = Some(web_context);
            *self.webview.borrow_mut() = Some(webview);
            Ok(())
        })
    }

    fn Unload(&self) -> Result<()> {
        // Dropping the WebView destroys the child window/controller.
        *self.webview.borrow_mut() = None;
        *self.web_context.borrow_mut() = None;
        *self.stream.borrow_mut() = None;
        Ok(())
    }

    fn SetFocus(&self) -> Result<()> {
        Ok(())
    }

    fn QueryFocus(&self) -> Result<HWND> {
        // The WebView2 child owns keyboard focus; report the preview's parent window.
        let p = self.parent.get();
        if p == 0 {
            Err(E_FAIL.into())
        } else {
            Ok(HWND(p as *mut c_void))
        }
    }

    fn TranslateAccelerator(&self, _pmsg: *const MSG) -> Result<()> {
        // S_FALSE: not handled here, let the host process the message.
        Err(Error::from(S_FALSE))
    }
}

impl UdfPreviewHandler_Impl {
    fn bounds(&self) -> Rect {
        let rc = *self.rect.borrow();
        // The host gives the rect in physical device pixels; pass it through as physical so
        // wry doesn't re-scale it by the monitor's DPI factor (which oversizes the webview on
        // a >100% display and clips its right/bottom).
        Rect {
            position: PhysicalPosition::new(rc.left, rc.top).into(),
            size: PhysicalSize::new((rc.right - rc.left).max(0), (rc.bottom - rc.top).max(0))
                .into(),
        }
    }

    fn apply_bounds(&self) {
        if let Some(wv) = self.webview.borrow().as_ref() {
            let _ = wv.set_bounds(self.bounds());
        }
    }
}

impl IObjectWithSite_Impl for UdfPreviewHandler_Impl {
    fn SetSite(&self, punksite: Option<&IUnknown>) -> Result<()> {
        *self.site.borrow_mut() = punksite.cloned();
        Ok(())
    }

    fn GetSite(&self, riid: *const GUID, ppvsite: *mut *mut c_void) -> Result<()> {
        unsafe {
            if ppvsite.is_null() {
                return Err(E_POINTER.into());
            }
            *ppvsite = std::ptr::null_mut();
            match self.site.borrow().as_ref() {
                Some(site) => site.query(riid, ppvsite).ok(),
                None => Err(E_FAIL.into()),
            }
        }
    }
}

impl IOleWindow_Impl for UdfPreviewHandler_Impl {
    fn GetWindow(&self) -> Result<HWND> {
        let p = self.parent.get();
        if p == 0 {
            Err(E_FAIL.into())
        } else {
            Ok(HWND(p as *mut c_void))
        }
    }

    fn ContextSensitiveHelp(&self, _fentermode: windows::Win32::Foundation::BOOL) -> Result<()> {
        Ok(())
    }
}
