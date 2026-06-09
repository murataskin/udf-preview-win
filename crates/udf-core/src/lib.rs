//! `udf-core` — cross-platform parser for UYAP `.udf` files plus two pure string
//! backends (HTML and RTF). No OS dependencies; everything here is unit-testable on
//! any platform.
//!
//! Pipeline: `UDF bytes -> parse -> Document -> { to_html | to_rtf }`.
//!
//! Ported faithfully from the authoritative macOS app
//! (<https://github.com/saidsurucu/udf-quicklook-extension>). See the design spec at
//! `docs/superpowers/specs/2026-06-09-udf-preview-win-design.md` for the correctness rules
//! that every module must uphold (rune = Rust `char`, slice-raw-then-clean,
//! `borderColor = 0` is opaque, lenient base64).

pub mod color;
pub mod html;
pub mod model;
pub mod parser;
pub mod rtf;
pub mod rune;
pub mod xml;
pub mod zip;

pub use model::Document;

use std::fmt;

/// Errors that can occur while parsing a `.udf` file. The parser must never panic —
/// every failure path returns one of these.
#[derive(Debug)]
pub enum UdfError {
    /// The bytes are not a valid ZIP archive.
    InvalidZip(String),
    /// The archive opened, but no `content.xml` entry was found (e.g. empty/corrupt file).
    MissingContentXml,
    /// `content.xml` could not be parsed as XML.
    InvalidXml(String),
    /// The XML parsed but the expected `<template>/<content>` structure was absent.
    MalformedDocument(String),
}

impl fmt::Display for UdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UdfError::InvalidZip(m) => write!(f, "invalid ZIP archive: {m}"),
            UdfError::MissingContentXml => write!(f, "no content.xml in archive"),
            UdfError::InvalidXml(m) => write!(f, "invalid content.xml: {m}"),
            UdfError::MalformedDocument(m) => write!(f, "malformed UDF document: {m}"),
        }
    }
}

impl std::error::Error for UdfError {}

/// Parse `.udf` file bytes into the [`Document`] model.
pub fn parse(bytes: &[u8]) -> Result<Document, UdfError> {
    parser::parse(bytes)
}

/// Render a parsed [`Document`] to a self-contained HTML string.
pub fn to_html(doc: &Document) -> String {
    html::to_html(doc)
}

/// Render a parsed [`Document`] to an RTF string (lean, first-page fidelity).
pub fn to_rtf(doc: &Document) -> String {
    rtf::to_rtf(doc)
}

/// Convenience: `.udf` bytes straight to HTML.
pub fn udf_to_html(bytes: &[u8]) -> Result<String, UdfError> {
    Ok(to_html(&parse(bytes)?))
}

/// Convenience: `.udf` bytes straight to RTF.
pub fn udf_to_rtf(bytes: &[u8]) -> Result<String, UdfError> {
    Ok(to_rtf(&parse(bytes)?))
}
