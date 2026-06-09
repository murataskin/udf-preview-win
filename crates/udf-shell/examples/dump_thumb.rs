//! Verification harness: render a `.udf`'s first-page thumbnail and write it as a BMP, so the
//! RichEdit->DIB path can be eyeballed without registering the DLL in Explorer.
//!
//!   cargo run -p udf-shell --example dump_thumb -- <file.udf> [out.bmp] [width]

#[cfg(not(windows))]
fn main() {
    eprintln!("dump_thumb is Windows-only.");
}

#[cfg(windows)]
fn main() {
    use windows::Win32::Graphics::Gdi::DeleteObject;

    let mut args = std::env::args().skip(1);
    let Some(input) = args.next() else {
        eprintln!("usage: dump_thumb <file.udf> [out.bmp] [width]");
        std::process::exit(2);
    };
    let out = args.next().unwrap_or_else(|| "thumb.bmp".to_string());
    let width: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(256);

    let bytes = std::fs::read(&input).expect("read .udf");
    let r = udf_shell::thumbnail::render_udf_dib(&bytes, width).expect("render thumbnail");

    // Copy the top-down BGRA bits out of the DIB section, then release the bitmap.
    let len = (r.width * r.height * 4) as usize;
    let pixels = unsafe { std::slice::from_raw_parts(r.bits, len).to_vec() };
    unsafe {
        let _ = DeleteObject(r.hbitmap);
    }

    write_bmp(&out, r.width, r.height, &pixels).expect("write bmp");
    println!("wrote {out} ({}x{})", r.width, r.height);
}

/// Write a 32-bit top-down BMP (negative height) from BGRA bytes.
#[cfg(windows)]
fn write_bmp(path: &str, width: i32, height: i32, bgra: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let pixel_bytes = (width * height * 4) as u32;
    let file_size = 14 + 40 + pixel_bytes;

    // BITMAPFILEHEADER
    f.write_all(b"BM")?;
    f.write_all(&file_size.to_le_bytes())?;
    f.write_all(&0u16.to_le_bytes())?; // reserved1
    f.write_all(&0u16.to_le_bytes())?; // reserved2
    f.write_all(&(14u32 + 40).to_le_bytes())?; // pixel data offset

    // BITMAPINFOHEADER (top-down: negative height)
    f.write_all(&40u32.to_le_bytes())?;
    f.write_all(&width.to_le_bytes())?;
    f.write_all(&(-height).to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // planes
    f.write_all(&32u16.to_le_bytes())?; // bpp
    f.write_all(&0u32.to_le_bytes())?; // BI_RGB
    f.write_all(&pixel_bytes.to_le_bytes())?;
    f.write_all(&2835u32.to_le_bytes())?; // x ppm (~72dpi)
    f.write_all(&2835u32.to_le_bytes())?; // y ppm
    f.write_all(&0u32.to_le_bytes())?; // colors used
    f.write_all(&0u32.to_le_bytes())?; // important

    f.write_all(bgra)?;
    Ok(())
}
