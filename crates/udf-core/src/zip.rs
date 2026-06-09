//! ZIP container handling. A `.udf` is a ZIP that (almost always) contains `content.xml`.
//! Some also carry `sign.sgn` (e-signature) or ODF-style entries (`mimetype`, `meta.xml`,
//! `META-INF/manifest.xml`) — all of which we ignore.

use crate::UdfError;
use std::io::Read;

/// Open the ZIP from raw bytes and return the bytes of `content.xml`.
///
/// Errors cleanly (never panics) on a non-ZIP input or a ZIP missing `content.xml`.
pub fn extract_content_xml(bytes: &[u8]) -> Result<Vec<u8>, UdfError> {
    let reader = std::io::Cursor::new(bytes);
    let mut archive =
        ::zip::ZipArchive::new(reader).map_err(|e| UdfError::InvalidZip(e.to_string()))?;

    let mut file = archive
        .by_name("content.xml")
        .map_err(|_| UdfError::MissingContentXml)?;

    let mut buf = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut buf)
        .map_err(|e| UdfError::InvalidZip(e.to_string()))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_zip_bytes_error_not_panic() {
        let err = extract_content_xml(b"not a zip at all").unwrap_err();
        matches!(err, UdfError::InvalidZip(_));
    }

    #[test]
    fn empty_bytes_error_not_panic() {
        assert!(extract_content_xml(&[]).is_err());
    }
}
