//! Diagnostic: instantiate the *registered* thumbnail handler through COM (CoCreateInstance),
//! feed it a `.udf` via an in-memory IStream, call GetThumbnail, and save the result. This
//! exercises the exact path Explorer uses (DLL load + class factory + interfaces) but in a
//! normal process, isolating code bugs from Explorer/surrogate integration issues.
//!
//!   cargo run -p udf-shell --example com_selftest -- <file.udf> [out.bmp]

#[cfg(not(windows))]
fn main() {
    eprintln!("com_selftest is Windows-only.");
}

#[cfg(windows)]
fn main() {
    if let Err(e) = run() {
        eprintln!("FAILED: {e} (0x{:08X})", e.code().0);
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run() -> windows::core::Result<()> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::IInitializeWithStream;
    use windows::Win32::UI::Shell::{IThumbnailProvider, SHCreateMemStream};

    let path = std::env::args()
        .nth(1)
        .expect("usage: com_selftest <file.udf> [out.bmp]");
    let out = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "selftest.bmp".to_string());
    let bytes = std::fs::read(&path).expect("read .udf");

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let stream = SHCreateMemStream(Some(&bytes)).expect("SHCreateMemStream");
        let init: IInitializeWithStream =
            CoCreateInstance(&udf_shell::THUMBNAIL_CLSID, None, CLSCTX_INPROC_SERVER)?;
        println!("CoCreateInstance OK (DLL loaded via COM)");
        init.Initialize(&stream, 0)?;
        println!("Initialize OK");

        let tp: IThumbnailProvider = init.cast()?;
        let mut hbmp = windows::Win32::Graphics::Gdi::HBITMAP::default();
        let mut alpha = windows::Win32::UI::Shell::WTS_ALPHATYPE::default();
        tp.GetThumbnail(256, &mut hbmp, &mut alpha)?;
        println!("GetThumbnail OK: hbmp={:?}, alpha={}", hbmp, alpha.0);

        // Read the HBITMAP pixels back and write a BMP.
        let mut bm = BITMAP::default();
        GetObjectW(
            hbmp,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut _ as *mut core::ffi::c_void),
        );
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let scanlines = GetDIBits(
            dc,
            hbmp,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
            &mut bi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        let _ = DeleteObject(hbmp);
        println!("GetDIBits scanlines={scanlines}, {w}x{h}");

        write_bmp(&out, w, h, &buf).expect("write bmp");
        println!("wrote {out}");
        Ok(())
    }
}

#[cfg(windows)]
fn write_bmp(path: &str, width: i32, height: i32, bgra: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let pixel_bytes = (width * height * 4) as u32;
    f.write_all(b"BM")?;
    f.write_all(&(14 + 40 + pixel_bytes).to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?;
    f.write_all(&(54u32).to_le_bytes())?;
    f.write_all(&40u32.to_le_bytes())?;
    f.write_all(&width.to_le_bytes())?;
    f.write_all(&(-height).to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&32u16.to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?;
    f.write_all(&pixel_bytes.to_le_bytes())?;
    f.write_all(&2835u32.to_le_bytes())?;
    f.write_all(&2835u32.to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?;
    f.write_all(bgra)?;
    Ok(())
}
