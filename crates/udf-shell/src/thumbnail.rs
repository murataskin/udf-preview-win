//! First-page thumbnail rendering: `.udf` -> RTF -> off-screen RICHEDIT50W -> a top-down
//! 32-bit DIB, downscaled to the shell-requested width.
//!
//! RichEdit is in-process and synchronous, so this works inside the sandboxed thumbnail COM
//! surrogate where WebView2 cannot reliably spawn its child processes. The produced bitmap is
//! opaque (`WTSAT_RGB`): GDI text rendering does not preserve the alpha byte, so we render
//! black text on a white page and let the shell ignore alpha.

use std::sync::Once;

use windows::core::{Result, PCWSTR};
use windows::Win32::Foundation::{E_FAIL, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDeviceCaps, ReleaseDC,
    SelectObject, SetStretchBltMode, StretchBlt, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HALFTONE, HBITMAP, HDC, LOGPIXELSX, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Controls::RichEdit::{
    CHARRANGE, EDITSTREAM, EM_FORMATRANGE, EM_STREAMIN, FORMATRANGE, MSFTEDIT_CLASS, SF_RTF,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, SendMessageW, ES_MULTILINE, WINDOW_EX_STYLE, WS_POPUP,
};

/// A4 dimensions used for the generic first-page page box.
const A4_WIDTH_IN: f64 = 8.27;
const A4_HEIGHT_IN: f64 = 11.69;
const TWIPS_PER_INCH: f64 = 1440.0;

static RICHEDIT_LOADED: Once = Once::new();

/// A rendered DIB. `bits` points into the DIB section and stays valid until the `hbitmap` is
/// deleted; the shell takes ownership of `hbitmap` (it must not be deleted by us in that path).
pub struct Rendered {
    pub hbitmap: HBITMAP,
    pub width: i32,
    pub height: i32,
    pub bits: *mut u8,
}

/// Render the first page of a `.udf` to an opaque HBITMAP `cx` pixels wide.
pub fn render_udf_thumbnail(udf_bytes: &[u8], cx: u32) -> Result<HBITMAP> {
    Ok(render_udf_dib(udf_bytes, cx)?.hbitmap)
}

/// Render and return the full [`Rendered`] (for examples/tests that read the pixels).
pub fn render_udf_dib(udf_bytes: &[u8], cx: u32) -> Result<Rendered> {
    let rtf = udf_core::udf_to_rtf(udf_bytes).map_err(|_| windows::core::Error::from(E_FAIL))?;
    render_rtf_first_page(&rtf, cx.max(1))
}

struct StreamCookie {
    data: *const u8,
    len: usize,
    pos: usize,
}

unsafe extern "system" fn stream_in_cb(
    cookie: usize,
    pb_buff: *mut u8,
    cb: i32,
    pcb: *mut i32,
) -> u32 {
    let c = &mut *(cookie as *mut StreamCookie);
    let remaining = c.len - c.pos;
    let n = remaining.min(cb.max(0) as usize);
    if n > 0 {
        std::ptr::copy_nonoverlapping(c.data.add(c.pos), pb_buff, n);
        c.pos += n;
    }
    *pcb = n as i32;
    0 // 0 = continue
}

fn render_rtf_first_page(rtf: &str, cx: u32) -> Result<Rendered> {
    unsafe {
        RICHEDIT_LOADED.call_once(|| {
            let _ = LoadLibraryW(PCWSTR(windows::core::w!("Msftedit.dll").as_ptr()));
        });

        let screen = GetDC(None);
        let dpi = GetDeviceCaps(screen, LOGPIXELSX).max(72);
        let page_w = (A4_WIDTH_IN * dpi as f64).round() as i32;
        let page_h = (A4_HEIGHT_IN * dpi as f64).round() as i32;

        // Full-resolution render target (white page).
        let render_dc = CreateCompatibleDC(screen);
        let (render_bmp, render_bits) = create_dib(render_dc, page_w, page_h)?;
        let old_render = SelectObject(render_dc, render_bmp);
        std::ptr::write_bytes(render_bits, 0xFF, (page_w * page_h * 4) as usize);

        // Off-screen RichEdit control.
        let hinstance = GetModuleHandleW(None)?;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            MSFTEDIT_CLASS,
            PCWSTR::null(),
            WS_POPUP | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(ES_MULTILINE as u32),
            0,
            0,
            page_w,
            page_h,
            None,
            None,
            hinstance,
            None,
        )?;

        // Stream the RTF in.
        let bytes = rtf.as_bytes();
        let mut cookie = StreamCookie {
            data: bytes.as_ptr(),
            len: bytes.len(),
            pos: 0,
        };
        let mut es = EDITSTREAM {
            dwCookie: &mut cookie as *mut _ as usize,
            dwError: 0,
            pfnCallback: Some(stream_in_cb),
        };
        SendMessageW(
            hwnd,
            EM_STREAMIN,
            WPARAM(SF_RTF as usize),
            LPARAM(&mut es as *mut _ as isize),
        );

        // Render the first page. rc/rcPage are in twips; RichEdit maps them to device pixels
        // via the target DC's LOGPIXELS, so the A4 twips box maps exactly onto our page DIB.
        let twips = RECT {
            left: 0,
            top: 0,
            right: (A4_WIDTH_IN * TWIPS_PER_INCH) as i32,
            bottom: (A4_HEIGHT_IN * TWIPS_PER_INCH) as i32,
        };
        let mut fr = FORMATRANGE {
            hdc: render_dc,
            hdcTarget: render_dc,
            rc: twips,
            rcPage: twips,
            chrg: CHARRANGE {
                cpMin: 0,
                cpMax: -1,
            },
        };
        SendMessageW(
            hwnd,
            EM_FORMATRANGE,
            WPARAM(1),
            LPARAM(&mut fr as *mut _ as isize),
        );
        // Free the formatting cache.
        SendMessageW(hwnd, EM_FORMATRANGE, WPARAM(0), LPARAM(0));

        // Downscale to the requested width, preserving A4 aspect.
        let cy = ((cx as f64) * page_h as f64 / page_w as f64)
            .round()
            .max(1.0) as i32;
        let thumb_dc = CreateCompatibleDC(screen);
        let (thumb_bmp, thumb_bits) = create_dib(thumb_dc, cx as i32, cy)?;
        let old_thumb = SelectObject(thumb_dc, thumb_bmp);
        std::ptr::write_bytes(thumb_bits, 0xFF, (cx as i32 * cy * 4) as usize);
        SetStretchBltMode(thumb_dc, HALFTONE);
        let _ = StretchBlt(
            thumb_dc, 0, 0, cx as i32, cy, render_dc, 0, 0, page_w, page_h, SRCCOPY,
        );

        // GDI text rendering and StretchBlt leave the DIB's alpha byte at 0. The shell treats
        // a 32-bit DIB as having alpha and composites alpha=0 onto black — turning the whole
        // thumbnail black. Force every pixel opaque so the rendered page shows through.
        let pixel_count = (cx as i32 * cy) as usize;
        for i in 0..pixel_count {
            *thumb_bits.add(i * 4 + 3) = 0xFF;
        }

        // Tear down everything except the returned thumbnail bitmap.
        SelectObject(thumb_dc, old_thumb);
        SelectObject(render_dc, old_render);
        let _ = DeleteDC(thumb_dc);
        let _ = DeleteObject(render_bmp);
        let _ = DeleteDC(render_dc);
        let _ = DestroyWindow(hwnd);
        ReleaseDC(None, screen);

        Ok(Rendered {
            hbitmap: thumb_bmp,
            width: cx as i32,
            height: cy,
            bits: thumb_bits,
        })
    }
}

unsafe fn create_dib(dc: HDC, w: i32, h: i32) -> Result<(HBITMAP, *mut u8)> {
    let bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // negative -> top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, None, 0)?;
    Ok((bmp, bits as *mut u8))
}

// `GetDC` lives in Gdi; import here to keep the use-list above tidy.
use windows::Win32::Graphics::Gdi::GetDC;
