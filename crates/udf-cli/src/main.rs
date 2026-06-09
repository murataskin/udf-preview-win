//! `udf-cli <file.udf> [--rtf] [-o out]` — converts a `.udf` to HTML (default) or RTF.
//!
//! Prefer `-o <file>` on Windows: it writes the exact UTF-8/ANSI bytes straight to disk. A
//! PowerShell redirect (`udf-cli x.udf > out.rtf`) re-encodes stdout as UTF-16LE with a BOM,
//! which browsers tolerate but **RTF readers (Word/RichEdit) cannot parse**. With `-o` the
//! shell never touches the bytes:
//!   `udf-cli x.udf -o out.html; start out.html`
//!   `udf-cli x.udf --rtf -o out.rtf; start out.rtf`

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut path: Option<String> = None;
    let mut out_path: Option<String> = None;
    let mut rtf = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--rtf" => rtf = true,
            "-o" | "--out" => match args.next() {
                Some(p) => out_path = Some(p),
                None => {
                    eprintln!("error: {arg} requires a file path");
                    return ExitCode::from(2);
                }
            },
            "-h" | "--help" => {
                eprintln!("usage: udf-cli <file.udf> [--rtf] [-o <out>]");
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("error: unknown flag `{other}`");
                eprintln!("usage: udf-cli <file.udf> [--rtf] [-o <out>]");
                return ExitCode::from(2);
            }
            other => {
                if path.is_some() {
                    eprintln!("error: more than one input file given");
                    return ExitCode::from(2);
                }
                path = Some(other.to_string());
            }
        }
    }

    let Some(path) = path else {
        eprintln!("usage: udf-cli <file.udf> [--rtf] [-o <out>]");
        return ExitCode::from(2);
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let result = if rtf {
        udf_core::udf_to_rtf(&bytes)
    } else {
        udf_core::udf_to_html(&bytes)
    };

    let out = match result {
        Ok(out) => out,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Write exact bytes. To a file when `-o` is given (correct on every shell); otherwise to
    // stdout as raw bytes (UTF-8) — note a PowerShell `>` redirect will still re-encode it.
    if let Some(out_file) = out_path {
        if let Err(e) = std::fs::write(&out_file, out.as_bytes()) {
            eprintln!("error: cannot write {out_file}: {e}");
            return ExitCode::FAILURE;
        }
    } else if let Err(e) = std::io::stdout().write_all(out.as_bytes()) {
        eprintln!("error: cannot write stdout: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
