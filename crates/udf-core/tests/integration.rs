//! Integration tests over whole `.udf` archives.
//!
//! No `.udf` files are committed (they may carry confidential content). Inputs are:
//!   * synthetic archives built in-memory from a `content.xml` string — these run everywhere,
//!     including CI, and exercise rune slicing, tables, and the negative paths deterministically.
//!   * an optional local `fixtures/sample.udf` (untracked) for richer assertions when present.
//!   * an optional `UDF_SAMPLE_DIR` folder swept for parse + render sanity over real files.

use std::io::Write;

use udf_core::model::{BlockElement, InlineElement};
use udf_core::{parse, to_html, to_rtf, udf_to_html, UdfError};

/// Wrap a `content.xml` string into a minimal `.udf` (ZIP) byte vector.
fn udf_with_content_xml(xml: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        w.start_file("content.xml", opts).unwrap();
        w.write_all(xml.as_bytes()).unwrap();
        w.finish().unwrap();
    }
    buf
}

/// Build a `.udf` ZIP whose single entry has the given name (used for negative tests).
fn udf_with_entry(name: &str, body: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        w.start_file(name, opts).unwrap();
        w.write_all(body).unwrap();
        w.finish().unwrap();
    }
    buf
}

/// `.udf` files are never committed (they may carry confidential content). When a local
/// `fixtures/sample.udf` is present this test runs detailed assertions over it; in CI, where
/// the file is absent, it skips. The synthetic in-memory tests below cover the core in CI.
#[test]
fn parses_local_sample_with_image() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/sample.udf");
    let Ok(bytes) = std::fs::read(path) else {
        eprintln!("fixtures/sample.udf absent — skipping local sample assertions");
        return;
    };
    let doc = parse(&bytes).expect("sample.udf should parse");

    assert_eq!(doc.format_id, "1.8");
    assert_eq!(doc.resolver, "hvl-default");
    assert_eq!(doc.pages.media_size_name, 1);
    assert!((doc.pages.left_margin - 42.51968).abs() < 0.01);
    assert_eq!(doc.styles.len(), 2);
    assert_eq!(doc.styles[0].name, "default");

    // 8 paragraphs. CDATA = "Sdfsdfsdfjklşlkj" + 3×U+200B + U+FFFC + U+200B + "DSFSDFDSF".
    assert_eq!(doc.body.len(), 8);

    let para = |i: usize| -> &udf_core::model::Paragraph {
        match &doc.body[i] {
            BlockElement::Paragraph(p) => p,
            _ => panic!("body[{i}] should be a paragraph"),
        }
    };
    let para_text = |p: &udf_core::model::Paragraph| -> String {
        p.runs
            .iter()
            .filter_map(|r| match r {
                InlineElement::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect()
    };

    // P0: four runs spanning offsets [0,9) -> "Sdfsdfsdf"; the [3,6) run is bold.
    let p0 = para(0);
    assert_eq!(p0.runs.len(), 4);
    assert_eq!(para_text(p0), "Sdfsdfsdf");
    let InlineElement::Text(bold_run) = &p0.runs[2] else {
        panic!("third run is text");
    };
    assert!(bold_run.style.bold, "[3,6) run is bold");

    // P1: real-file rune slice of "jklşlkj" — the 'ş' (U+015F) is one rune, not split.
    assert_eq!(para_text(para(1)), "jklşlkj");

    // P2/P3/P4: zero-width-space-only paragraphs collapse to empty (no runs).
    assert!(para(2).runs.is_empty());
    assert!(para(3).runs.is_empty());
    assert!(para(4).runs.is_empty());

    // P5: the image paragraph.
    let p5 = para(5);
    assert_eq!(p5.runs.len(), 1);
    let InlineElement::Image(img) = &p5.runs[0] else {
        panic!("expected an image run");
    };
    assert!(
        img.data.starts_with("iVBOR"),
        "PNG base64 payload preserved"
    );

    // P7: last paragraph "DSFSDFDSF".
    assert_eq!(para_text(para(7)), "DSFSDFDSF");
}

#[test]
fn rune_offsets_slice_turkish_text_through_parser() {
    // CDATA holds Turkish text with multibyte scalars; the two content runs reference it by
    // rune offset/length. "Merhaba Dünya" — runes: M0 e1 r2 h3 a4 b5 a6 (space)7 D8 ü9 n10 y11 a12
    let xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<template format_id="1.8">
<content><![CDATA[Merhaba Dünya]]></content>
<properties><pageFormat mediaSizeName="1" /></properties>
<elements resolver="hvl-default">
<paragraph Alignment="0"><content startOffset="0" length="7" family="Arial" size="14" bold="true" /><content startOffset="8" length="5" family="Arial" size="14" /></paragraph>
</elements>
</template>"#;
    let doc = parse(&udf_with_content_xml(xml)).expect("should parse");
    assert_eq!(doc.body.len(), 1);
    let BlockElement::Paragraph(p) = &doc.body[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(p.runs.len(), 2);
    let InlineElement::Text(r0) = &p.runs[0] else {
        panic!("text run");
    };
    let InlineElement::Text(r1) = &p.runs[1] else {
        panic!("text run");
    };
    assert_eq!(r0.text, "Merhaba");
    assert_eq!(r0.style.font_family, "Arial");
    assert_eq!(r0.style.font_size, 14.0);
    assert!(r0.style.bold);
    // "Dünya" — the multibyte 'ü' must be sliced as a single rune, not split.
    assert_eq!(r1.text, "Dünya");
    assert!(!r1.style.bold);
}

#[test]
fn empty_paragraph_placeholder_captures_font_and_no_runs() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<template format_id="1.8">
<content><![CDATA[X]]></content>
<elements resolver="hvl-default">
<paragraph Alignment="0"><content startOffset="0" length="1" family="Courier New" size="10" /></paragraph>
</elements>
</template>"#;
    let doc = parse(&udf_with_content_xml(xml)).expect("should parse");
    let BlockElement::Paragraph(p) = &doc.body[0] else {
        panic!("paragraph");
    };
    assert!(p.runs.is_empty(), "placeholder yields no runs");
    let s = p
        .empty_content_style
        .as_ref()
        .expect("captured empty style");
    assert_eq!(s.font_family, "Courier New");
    assert_eq!(s.font_size, 10.0);
}

#[test]
fn parses_nested_table_with_inherited_border() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<template format_id="1.8">
<content><![CDATA[AB]]></content>
<elements resolver="hvl-default">
<table tableName="Sabit" columnCount="1" border="borderTable" borderStyle="borderStyle-solid" borderWidth="1.0" borderColor="0" borderSpec="15" columnSpans="100">
<row rowName="row1" rowType="dataRow">
<cell>
<paragraph Alignment="0"><content startOffset="0" length="1" family="Arial" size="12" /></paragraph>
<table tableName="Sabit" columnCount="1" columnSpans="100">
<row><cell><paragraph Alignment="0"><content startOffset="1" length="1" family="Arial" size="12" /></paragraph></cell></row>
</table>
</cell>
</row>
</table>
</elements>
</template>"#;
    let doc = parse(&udf_with_content_xml(xml)).expect("should parse");
    let BlockElement::Table(t) = &doc.body[0] else {
        panic!("table");
    };
    assert_eq!(t.border_color, Some(0));
    let cell = &t.rows[0].cells[0];
    // Cell defines no border of its own -> inherits the table-level border.
    assert_eq!(cell.border.name, "borderTable");
    assert_eq!(cell.border_color, 0); // 0 here means opaque black at render time
                                      // The cell contains a paragraph then a nested table.
    assert_eq!(cell.content.len(), 2);
    assert!(matches!(cell.content[1], BlockElement::Table(_)));
}

#[test]
fn missing_content_xml_is_clean_error() {
    // ODF-style packaging: a ZIP without content.xml.
    let bytes = udf_with_entry("mimetype", b"application/vnd.oasis.opendocument.text");
    match parse(&bytes) {
        Err(UdfError::MissingContentXml) => {}
        other => panic!("expected MissingContentXml, got {other:?}"),
    }
}

#[test]
fn corrupt_zip_is_clean_error() {
    let bytes = b"this is definitely not a zip archive";
    assert!(matches!(parse(bytes), Err(UdfError::InvalidZip(_))));
}

#[test]
fn empty_bytes_is_clean_error() {
    assert!(parse(&[]).is_err());
}

#[test]
fn xml_without_template_root_is_clean_error() {
    let bytes = udf_with_content_xml("<?xml version=\"1.0\"?><notTemplate/>");
    assert!(matches!(parse(&bytes), Err(UdfError::MalformedDocument(_))));
}

#[test]
fn html_is_xml_well_formed_for_sample() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/sample.udf");
    let Ok(bytes) = std::fs::read(path) else {
        eprintln!("fixtures/sample.udf absent — skipping");
        return;
    };
    let html = udf_to_html(&bytes).unwrap();
    assert!(html.contains("<img"), "sample renders its image");
    assert_wellformed_html(&html);
}

#[test]
fn empty_doc_html_and_rtf_are_sane() {
    let doc = udf_core::model::Document::default();
    let html = to_html(&doc);
    assert_wellformed_html(&html);
    let rtf = to_rtf(&doc);
    assert!(braces_balanced(&rtf), "RTF braces balanced");
}

/// The HTML body is XML-parseable once the HTML5 doctype is stripped (we self-close void
/// elements specifically so this holds), and brace/markup bugs surface as parse errors.
fn assert_wellformed_html(html: &str) {
    let body = html
        .strip_prefix("<!doctype html>")
        .unwrap_or(html)
        .trim_start();
    if let Err(e) = roxmltree::Document::parse(body) {
        panic!("generated HTML is not well-formed XML: {e}");
    }
}

fn braces_balanced(s: &str) -> bool {
    let mut depth: i32 = 0;
    let mut prev_backslash = false;
    for c in s.chars() {
        match c {
            '{' if !prev_backslash => depth += 1,
            '}' if !prev_backslash => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
        prev_backslash = c == '\\' && !prev_backslash;
    }
    depth == 0
}

/// Opt-in sweep over a folder of real `.udf` documents. Set `UDF_SAMPLE_DIR`; every file must
/// parse without panicking AND produce well-formed HTML + brace-balanced RTF. Skipped when
/// the env var is unset, so the committed test suite never depends on private files.
#[test]
fn sample_dir_sweep_parses_and_renders() {
    let Ok(dir) = std::env::var("UDF_SAMPLE_DIR") else {
        eprintln!("UDF_SAMPLE_DIR unset — skipping sample-dir sweep");
        return;
    };
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).expect("UDF_SAMPLE_DIR should be readable") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("udf") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        match parse(&bytes) {
            Ok(doc) => {
                let html = to_html(&doc);
                if let Err(e) = roxmltree::Document::parse(
                    html.strip_prefix("<!doctype html>")
                        .unwrap_or(&html)
                        .trim_start(),
                ) {
                    panic!("{name}: HTML not well-formed: {e}");
                }
                assert!(braces_balanced(&to_rtf(&doc)), "{name}: RTF unbalanced");
            }
            Err(e) => panic!("{name}: failed to parse: {e}"),
        }
        count += 1;
    }
    eprintln!("swept {count} .udf files under {dir}");
    assert!(count > 0, "expected at least one .udf in UDF_SAMPLE_DIR");
}
