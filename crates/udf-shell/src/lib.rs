//! Phase 2 (Windows) — COM in-process DLL exposing the `.udf` shell handlers:
//!   * `IInitializeWithStream` + `IThumbnailProvider` — first page rendered to a DIB via RTF
//!     + an off-screen RICHEDIT50W control (works inside the locked-down thumbnail surrogate).
//!   * `IPreviewHandler` + `IObjectWithSite` + `IOleWindow` — the HTML hosted in a WebView2
//!     child window in the Preview Pane.
//!
//! Every COM method that parses/renders is wrapped in `catch_unwind` (a panic crossing into
//! `explorer.exe`/`prevhost.exe` is UB). CLSIDs are fixed; registration is per-user under
//! `HKCU\Software\Classes` (no admin).
//!
//! The crate also builds as an `rlib` so `examples/` and tests can call the renderer directly
//! — letting us verify the produced bitmap/HTML without registering the DLL in Explorer.

#![cfg(windows)]
// COM ABI methods take raw pointers whose validity is guaranteed by the COM contract, not by
// Rust's `unsafe`. The functions deref them deliberately; the lint doesn't fit the COM model.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod thumbnail;

mod com;
mod preview;
mod registry;

pub use com::{
    DllCanUnloadNow, DllGetClassObject, DllRegisterServer, DllUnregisterServer, PREVIEW_CLSID,
    THUMBNAIL_CLSID,
};
