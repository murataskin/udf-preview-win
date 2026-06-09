//! Diagnostic: ask the *shell* for a file's thumbnail exactly the way Explorer does, via
//! `IShellItemImageFactory::GetImage(SIIGBF_THUMBNAILONLY)`. This exercises the full handler
//! resolution (ProgID / SystemFileAssociations / extension) AND the thumbnail surrogate, so a
//! success here means our registered handler is actually wired into the shell.
//!
//!   cargo run -p udf-shell --example shell_thumb -- <file.udf> [out.bmp]

#[cfg(not(windows))]
fn main() {
    eprintln!("shell_thumb is Windows-only.");
}

#[cfg(windows)]
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: shell_thumb <file.udf> [out.bmp]");
    let out = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "shellthumb.bmp".to_string());

    match run(&path, &out) {
        Ok(()) => println!("shell returned a thumbnail -> {out}"),
        Err(e) => {
            eprintln!("shell GetImage FAILED: {e} (0x{:08X})", e.code().0);
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
fn run(path: &str, out: &str) -> windows::core::Result<()> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(&HSTRING::from(path), None)?;
        let hbmp = factory.GetImage(SIZE { cx: 256, cy: 256 }, SIIGBF_THUMBNAILONLY)?;

        // Save it so the rendering can be eyeballed too.
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
        GetDIBits(
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
        println!("thumbnail {w}x{h}");
        write_bmp(out, w, h, &buf).expect("write bmp");
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
    f.write_all(&54u32.to_le_bytes())?;
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
