//! COM plumbing: class factory, the thumbnail provider object, and the DLL entry points.
//!
//! Every COM method body that runs UDF parsing/rendering is wrapped in `catch_unwind` — a
//! panic unwinding across the COM ABI into `explorer.exe`/the thumbnail surrogate is UB.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, Ordering};

use windows::core::{implement, IUnknown, Interface, Result, GUID, HRESULT, PCWSTR};
use windows::Win32::Foundation::{
    BOOL, CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_POINTER, S_FALSE, S_OK,
};
use windows::Win32::Graphics::Gdi::HBITMAP;
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl, IStream, STREAM_SEEK_SET};
use windows::Win32::UI::Shell::PropertiesSystem::{
    IInitializeWithFile, IInitializeWithFile_Impl, IInitializeWithStream,
    IInitializeWithStream_Impl,
};
use windows::Win32::UI::Shell::{
    IThumbnailProvider, IThumbnailProvider_Impl, WTSAT_RGB, WTS_ALPHATYPE,
};

/// CLSID of the thumbnail provider.
pub const THUMBNAIL_CLSID: GUID = GUID::from_u128(0x7F3D9A21_4C8B_4E1A_9F2D_1A2B3C4D5E61);
/// CLSID of the preview handler.
pub const PREVIEW_CLSID: GUID = GUID::from_u128(0x7F3D9A21_4C8B_4E1A_9F2D_1A2B3C4D5E62);

/// Count of live objects + server locks; the DLL may unload only when this is zero.
static LOCK_COUNT: AtomicI32 = AtomicI32::new(0);

pub(crate) struct DllGuard;
impl Default for DllGuard {
    fn default() -> Self {
        LOCK_COUNT.fetch_add(1, Ordering::SeqCst);
        DllGuard
    }
}
impl Drop for DllGuard {
    fn drop(&mut self) {
        LOCK_COUNT.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(crate) fn guard_com<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => Err(windows::core::Error::from(E_FAIL)),
    }
}

pub(crate) unsafe fn read_all_stream(stream: &IStream) -> Result<Vec<u8>> {
    let _ = stream.Seek(0, STREAM_SEEK_SET, None);
    let mut out = Vec::new();
    let mut buf = [0u8; 65536];
    loop {
        let mut read: u32 = 0;
        let _ = stream.Read(
            buf.as_mut_ptr() as *mut c_void,
            buf.len() as u32,
            Some(&mut read),
        );
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Thumbnail provider
// ---------------------------------------------------------------------------

#[implement(IInitializeWithStream, IInitializeWithFile, IThumbnailProvider)]
#[derive(Default)]
pub struct UdfThumbnailProvider {
    stream: RefCell<Option<IStream>>,
    file_path: RefCell<Option<String>>,
    _guard: DllGuard,
}

impl IInitializeWithStream_Impl for UdfThumbnailProvider_Impl {
    fn Initialize(&self, pstream: Option<&IStream>, _grfmode: u32) -> Result<()> {
        *self.stream.borrow_mut() = pstream.cloned();
        *self.file_path.borrow_mut() = None;
        Ok(())
    }
}

impl IInitializeWithFile_Impl for UdfThumbnailProvider_Impl {
    fn Initialize(&self, pszfilepath: &PCWSTR, _grfmode: u32) -> Result<()> {
        let path = unsafe { pszfilepath.to_string().unwrap_or_default() };
        *self.file_path.borrow_mut() = Some(path);
        *self.stream.borrow_mut() = None;
        Ok(())
    }
}

impl IThumbnailProvider_Impl for UdfThumbnailProvider_Impl {
    fn GetThumbnail(
        &self,
        cx: u32,
        phbmp: *mut HBITMAP,
        pdwalpha: *mut WTS_ALPHATYPE,
    ) -> Result<()> {
        guard_com(|| unsafe {
            if phbmp.is_null() || pdwalpha.is_null() {
                return Err(E_POINTER.into());
            }
            *phbmp = HBITMAP::default();
            *pdwalpha = WTSAT_RGB;
            
            let bytes = if let Some(stream) = self.stream.borrow().clone() {
                read_all_stream(&stream)?
            } else if let Some(path) = self.file_path.borrow().clone() {
                std::fs::read(&path).map_err(|_| windows::core::Error::from(E_FAIL))?
            } else {
                return Err(windows::core::Error::from(E_FAIL));
            };

            *phbmp = crate::thumbnail::render_udf_thumbnail(&bytes, cx)?;
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// Class factory
// ---------------------------------------------------------------------------

#[implement(IClassFactory)]
struct ClassFactory {
    clsid: GUID,
}

impl IClassFactory_Impl for ClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        unsafe {
            if ppvobject.is_null() {
                return Err(E_POINTER.into());
            }
            *ppvobject = std::ptr::null_mut();
            if punkouter.is_some() {
                return Err(CLASS_E_NOAGGREGATION.into());
            }
            let unknown: IUnknown = if self.clsid == THUMBNAIL_CLSID {
                UdfThumbnailProvider::default().into()
            } else if self.clsid == PREVIEW_CLSID {
                crate::preview::UdfPreviewHandler::default().into()
            } else {
                return Err(CLASS_E_CLASSNOTAVAILABLE.into());
            };
            unknown.query(riid, ppvobject).ok()
        }
    }

    fn LockServer(&self, flock: BOOL) -> Result<()> {
        if flock.as_bool() {
            LOCK_COUNT.fetch_add(1, Ordering::SeqCst);
        } else {
            LOCK_COUNT.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DLL entry points
// ---------------------------------------------------------------------------

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    unsafe {
        if ppv.is_null() || rclsid.is_null() || riid.is_null() {
            return E_POINTER;
        }
        *ppv = std::ptr::null_mut();
        let clsid = *rclsid;
        if clsid != THUMBNAIL_CLSID && clsid != PREVIEW_CLSID {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = ClassFactory { clsid }.into();
        factory.query(riid, ppv)
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if LOCK_COUNT.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllRegisterServer() -> HRESULT {
    // No-op. Manual registration via PowerShell installer is more robust for 
    // arch-specific paths and permission management.
    S_OK
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllUnregisterServer() -> HRESULT {
    S_OK
}
