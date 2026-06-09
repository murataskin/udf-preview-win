//! HTML backend (primary) — maps the [`Document`] model to self-contained HTML with inline
//! CSS and `data:` image URIs, rendered by WebView2 in the preview pane and viewer.
//!
//! The element-to-formatting mapping mirrors `DocumentToAttributedString.swift`: paragraph
//! alignment/indent/spacing/line-height, run font/weight/style/decoration/colour, table
//! borders driven by `borderSpec` bits with the `borderColor = 0` opaque rule, images at both
//! sites, and list markers per `UDFListMapping`. Lists are rendered the way the reference
//! actually renders them — the marker glyph/number injected as text (not browser `<ol>`),
//! since several UYAP marker types have no CSS list-style equivalent and numbering is keyed
//! per `listId`.
//!
//! Units: UDF measurements are points; we emit `pt` throughout so the layout shares the
//! reference's coordinate space. The browser does wrapping, fonts, and text rendering.

use std::collections::HashMap;

use crate::color::{border_css_hex, css_hex};
use crate::model::*;

/// Render a [`Document`] to a self-contained HTML string.
pub fn to_html(doc: &Document) -> String {
    let mut w = HtmlWriter::default();
    w.document(doc);
    w.out
}

#[derive(Default)]
struct HtmlWriter {
    out: String,
    /// Running item number per `listId` for numbered markers; `None` key is the anonymous bucket.
    list_counters: HashMap<Option<i32>, usize>,
}

impl HtmlWriter {
    fn document(&mut self, doc: &Document) {
        // Page geometry: A4 portrait 595.28 x 841.89 pt; swap for landscape.
        let (mut pw, mut ph) = (595.28_f64, 841.89_f64);
        if doc.pages.paper_orientation == 2 {
            std::mem::swap(&mut pw, &mut ph);
        }
        let m = &doc.pages;

        self.out.push_str(
            "<!doctype html><html lang=\"tr\"><head><meta charset=\"utf-8\"/>\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/>\
<style>",
        );
        self.out.push_str(BASE_CSS);
        self.out.push_str("</style></head><body>");

        self.out.push_str(&format!(
            "<div class=\"udf-page\" style=\"width:{}pt;min-height:{}pt;padding:{}pt {}pt {}pt {}pt;\">",
            num(pw),
            num(ph),
            num(m.top_margin),
            num(m.right_margin),
            num(m.bottom_margin),
            num(m.left_margin),
        ));

        self.blocks(&doc.header);
        self.blocks(&doc.body);
        self.blocks(&doc.footer);

        self.out.push_str("</div></body></html>");
    }

    fn blocks(&mut self, blocks: &[BlockElement]) {
        for block in blocks {
            match block {
                BlockElement::Paragraph(p) => self.paragraph(p),
                BlockElement::Table(t) => self.table(t),
                BlockElement::PageBreak => self
                    .out
                    .push_str("<div class=\"udf-pagebreak\">──────── Sayfa Sonu ────────</div>"),
            }
        }
    }

    fn paragraph(&mut self, p: &Paragraph) {
        let mut style = String::new();
        style.push_str(match p.alignment {
            Alignment::Left => "text-align:left;",
            Alignment::Center => "text-align:center;",
            Alignment::Right => "text-align:right;",
            Alignment::Justify => "text-align:justify;",
        });
        if p.space_before != 0.0 {
            style.push_str(&format!("margin-top:{}pt;", num(p.space_before)));
        }
        if p.space_after != 0.0 {
            style.push_str(&format!("margin-bottom:{}pt;", num(p.space_after)));
        }
        if p.line_spacing > 0.0 {
            style.push_str(&format!("line-height:{};", num(1.0 + p.line_spacing)));
        }

        // Marker prefix + indentation for list paragraphs (marker injected as text).
        let mut marker_html = String::new();
        if let Some(list) = &p.list {
            let indent = ((list.level.max(0) + 1) as f64) * 18.0 + p.left_indent;
            style.push_str(&format!(
                "margin-left:{}pt;text-indent:{}pt;",
                num(indent),
                num(-18.0)
            ));
            if p.right_indent > 0.0 {
                style.push_str(&format!("margin-right:{}pt;", num(p.right_indent)));
            }
            let marker = self.list_marker(list);
            marker_html = format!("<span class=\"udf-marker\">{}</span>", escape_text(&marker));
        } else {
            if p.left_indent != 0.0 {
                style.push_str(&format!("margin-left:{}pt;", num(p.left_indent)));
            }
            if p.right_indent != 0.0 {
                style.push_str(&format!("margin-right:{}pt;", num(p.right_indent)));
            }
            if p.first_line_indent != 0.0 {
                style.push_str(&format!("text-indent:{}pt;", num(p.first_line_indent)));
            }
        }

        self.out.push_str(&format!("<p style=\"{style}\">"));
        self.out.push_str(&marker_html);
        if p.runs.is_empty() {
            // Preserve the blank line's height.
            self.out.push_str("<br/>");
        } else {
            for run in &p.runs {
                self.run(run);
            }
        }
        self.out.push_str("</p>");
    }

    fn run(&mut self, run: &InlineElement) {
        match run {
            InlineElement::Text(t) => {
                let style = span_style(&t.style);
                if style.is_empty() {
                    self.out.push_str(&escape_text(&t.text));
                } else {
                    self.out.push_str(&format!(
                        "<span style=\"{}\">{}</span>",
                        style,
                        escape_text(&t.text)
                    ));
                }
            }
            InlineElement::Tab(_) => self.out.push('\t'),
            InlineElement::Image(img) => self.out.push_str(&image_tag(img)),
        }
    }

    fn table(&mut self, t: &Table) {
        self.out.push_str("<table class=\"udf-table\" style=\"");
        if let Some(align) = &t.align {
            match align.as_str() {
                "center" => self.out.push_str("margin-left:auto;margin-right:auto;"),
                "right" => self.out.push_str("margin-left:auto;"),
                _ => {}
            }
        }
        self.out.push_str("\"><tbody>");
        for row in &t.rows {
            self.out.push_str("<tr>");
            let total: f64 = row
                .cells
                .iter()
                .map(|c| c.width.unwrap_or((c.colspan.max(1) as f64) * 100.0))
                .sum::<f64>()
                .max(1.0);
            for cell in &row.cells {
                self.cell(cell, total, &row.height_min);
            }
            self.out.push_str("</tr>");
        }
        self.out.push_str("</tbody></table>");
    }

    fn cell(&mut self, cell: &TableCell, row_total: f64, height_min: &Option<f64>) {
        let mut style = String::from("padding:3pt;");
        style.push_str(match cell.vertical_align {
            VerticalAlignment::Top => "vertical-align:top;",
            VerticalAlignment::Middle => "vertical-align:middle;",
            VerticalAlignment::Bottom => "vertical-align:bottom;",
        });

        if let Some(w) = cell.width {
            let pct = (w / row_total * 100.0).round();
            style.push_str(&format!("width:{}%;", num(pct)));
        }
        if let Some(hm) = height_min {
            if *hm > 0.0 {
                style.push_str(&format!("height:{}pt;", num(*hm)));
            }
        }
        // Fill colour (skip white / -1, matching the reference).
        if cell.fill_color != 16777215 && cell.fill_color != -1 {
            style.push_str(&format!("background-color:{};", css_hex(cell.fill_color)));
        }
        // Borders: visibility from borderStyle + borderSpec bits, colour opaque
        // (borderColor=0 -> opaque black). top=1, right=2, bottom=4, left=8.
        let draw = cell.border.style != "borderStyle-none";
        let color = border_css_hex(cell.border_color);
        let width = num(cell.border_width);
        let edge = |bit: i32, name: &str| -> String {
            if draw && (cell.border_spec & bit) != 0 {
                format!("border-{name}:{width}pt solid {color};")
            } else {
                format!("border-{name}:none;")
            }
        };
        style.push_str(&edge(1, "top"));
        style.push_str(&edge(2, "right"));
        style.push_str(&edge(4, "bottom"));
        style.push_str(&edge(8, "left"));

        self.out.push_str(&format!("<td style=\"{style}\">"));
        if cell.content.is_empty() {
            self.out.push_str("<br/>");
        } else {
            self.blocks(&cell.content);
        }
        self.out.push_str("</td>");
    }

    // MARK: - List markers (mirrors UDFListMapping + ListCounterState)

    fn list_marker(&mut self, list: &ListProperties) -> String {
        match list.list_type {
            ListType::Bulleted => bullet_glyph(list.bullet_type.as_deref()),
            ListType::Numbered => {
                let counter = self.list_counters.entry(list.list_id).or_insert(0);
                *counter += 1;
                number_marker(list.number_type.as_deref(), *counter)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Run styling
// ---------------------------------------------------------------------------

fn span_style(s: &TextStyle) -> String {
    let d = TextStyle::default();
    let mut style = String::new();
    if !s.font_family.is_empty() {
        style.push_str(&format!("font-family:{};", css_font_family(&s.font_family)));
    }
    if s.font_size > 0.0 {
        style.push_str(&format!("font-size:{}pt;", num(s.font_size)));
    }
    if s.bold {
        style.push_str("font-weight:bold;");
    }
    if s.italic {
        style.push_str("font-style:italic;");
    }
    match (s.underline, s.strikethrough) {
        (true, true) => style.push_str("text-decoration:underline line-through;"),
        (true, false) => style.push_str("text-decoration:underline;"),
        (false, true) => style.push_str("text-decoration:line-through;"),
        (false, false) => {}
    }
    // Foreground: emit unless it is the default opaque black.
    if s.color != d.color {
        style.push_str(&format!("color:{};", css_hex(s.color)));
    }
    // Background highlight: emit only when not the default (-1 = white/none).
    if s.background_color != -1 {
        style.push_str(&format!(
            "background-color:{};",
            css_hex(s.background_color)
        ));
    }
    style
}

fn image_tag(img: &ImageRun) -> String {
    // Lenient base64: strip ALL whitespace before emitting (never drop the image).
    let cleaned: String = img.data.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() {
        return String::new();
    }
    let mime = if cleaned.starts_with("/9j/") {
        "jpeg"
    } else if cleaned.starts_with("R0lGOD") {
        "gif"
    } else {
        "png" // PNG ("iVBOR...") and the safe default
    };
    let mut dim = String::new();
    if img.width > 0.0 {
        dim.push_str(&format!("width:{}pt;", num(img.width)));
    }
    if img.height > 0.0 {
        dim.push_str(&format!("height:{}pt;", num(img.height)));
    }
    format!("<img alt=\"\" style=\"{dim}\" src=\"data:image/{mime};base64,{cleaned}\"/>")
}

// ---------------------------------------------------------------------------
// List marker formatting (mirrors UDFListMapping)
// ---------------------------------------------------------------------------

fn bullet_glyph(bullet_type: Option<&str>) -> String {
    let g = match bullet_type.unwrap_or("BULLET_TYPE_ELLIPSE") {
        "BULLET_TYPE_RECTANGLE" => '\u{25A0}',
        "BULLET_TYPE_RECTANGLE_D" => '\u{25A1}',
        "BULLET_TYPE_DIAMOND" => '\u{25C6}',
        "BULLET_TYPE_DIAMOND_2" => '\u{25C7}',
        "BULLET_TYPE_ARROW" => '\u{27A2}',
        "BULLET_TYPE_TRIANGLE" => '\u{25BA}',
        _ => '\u{2022}',
    };
    g.to_string()
}

fn number_marker(number_type: Option<&str>, n: usize) -> String {
    match number_type.unwrap_or("NUMBER_TYPE_NUMBER_DOT") {
        "NUMBER_TYPE_NUMBER_DOT" | "NUMBER_TYPE_NUMBER_DOT_2" | "NUMBER_TYPE_NUMBER_DOT_3" => {
            format!("{n}.")
        }
        "NUMBER_TYPE_NUMBER_PARANTHESE" | "NUMBER_TYPE_NUMBER_PARANTHESE_2" => format!("{n})"),
        "NUMBER_TYPE_NUMBER_TRE" => format!("{n}"),
        "NUMBER_TYPE_ROMAN_BIG_DOT" => format!("{}.", to_roman(n, true)),
        "NUMBER_TYPE_ROMAN_SMALL_DOT" => format!("{}.", to_roman(n, false)),
        "NUMBER_TYPE_CHAR_BIG_DOT" | "NUMBER_TYPE_CHAR_BIG_2" => format!("{}.", to_alpha(n, true)),
        "NUMBER_TYPE_CHAR_SMALL_DOT" => format!("{}.", to_alpha(n, false)),
        "NUMBER_TYPE_CHAR_SMALL_PARANTHESE" => format!("{})", to_alpha(n, false)),
        _ => format!("{n}."),
    }
}

fn to_roman(mut n: usize, upper: bool) -> String {
    if n == 0 {
        return String::new();
    }
    const TABLE: [(usize, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut s = String::new();
    for (v, sym) in TABLE {
        while n >= v {
            s.push_str(sym);
            n -= v;
        }
    }
    if upper {
        s.to_uppercase()
    } else {
        s
    }
}

fn to_alpha(mut n: usize, upper: bool) -> String {
    // Spreadsheet-style: 1->a, 26->z, 27->aa.
    let mut s = String::new();
    while n > 0 {
        n -= 1;
        s.insert(0, (b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    if upper {
        s.to_uppercase()
    } else {
        s
    }
}

// ---------------------------------------------------------------------------
// Escaping + number formatting
// ---------------------------------------------------------------------------

/// Escape text for HTML element content. Significant spaces/newlines are preserved by the
/// `white-space: pre-wrap` on paragraphs, so we keep them verbatim here.
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Build a CSS `font-family` value, single-quoted so it nests inside the double-quoted HTML
/// `style="..."` attribute without closing it. Single quotes / backslashes in the name are
/// CSS-escaped.
fn css_font_family(family: &str) -> String {
    let escaped = family.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

/// Format an f64 for CSS, dropping a trailing `.0` (Rust's `Display` already does this).
fn num(x: f64) -> String {
    let r = (x * 100.0).round() / 100.0; // clamp to 2 decimals to avoid float noise
    format!("{r}")
}

const BASE_CSS: &str = "\
html,body{margin:0;padding:0;}\
body{background:#e9e9ee;padding:16pt 0;}\
.udf-page{background:#fff;margin:0 auto;box-sizing:border-box;\
box-shadow:0 1pt 6pt rgba(0,0,0,.25);color:#000;}\
.udf-page p{margin:0;white-space:pre-wrap;overflow-wrap:break-word;tab-size:8;}\
.udf-marker{display:inline-block;min-width:14pt;}\
.udf-table{border-collapse:collapse;table-layout:fixed;width:100%;}\
.udf-table td{vertical-align:top;}\
.udf-pagebreak{text-align:center;color:#888;margin:6pt 0;white-space:pre;}\
img{max-width:100%;height:auto;}";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman_and_alpha_markers() {
        assert_eq!(to_roman(4, true), "IV");
        assert_eq!(to_roman(9, false), "ix");
        assert_eq!(to_roman(2024, true), "MMXXIV");
        assert_eq!(to_alpha(1, true), "A");
        assert_eq!(to_alpha(26, false), "z");
        assert_eq!(to_alpha(27, false), "aa");
    }

    #[test]
    fn number_marker_types() {
        assert_eq!(number_marker(Some("NUMBER_TYPE_NUMBER_DOT"), 3), "3.");
        assert_eq!(
            number_marker(Some("NUMBER_TYPE_NUMBER_PARANTHESE"), 3),
            "3)"
        );
        assert_eq!(number_marker(Some("NUMBER_TYPE_ROMAN_BIG_DOT"), 4), "IV.");
        assert_eq!(
            number_marker(Some("NUMBER_TYPE_CHAR_SMALL_PARANTHESE"), 1),
            "a)"
        );
        assert_eq!(number_marker(None, 7), "7.");
    }

    #[test]
    fn numbered_counter_keyed_by_list_id() {
        let mut w = HtmlWriter::default();
        let list = |id: Option<i32>| ListProperties {
            list_type: ListType::Numbered,
            number_type: Some("NUMBER_TYPE_NUMBER_DOT".to_string()),
            bullet_type: None,
            level: 0,
            list_id: id,
        };
        assert_eq!(w.list_marker(&list(Some(5))), "1.");
        assert_eq!(w.list_marker(&list(Some(5))), "2.");
        assert_eq!(w.list_marker(&list(Some(9))), "1."); // different list id restarts
        assert_eq!(w.list_marker(&list(Some(5))), "3.");
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(escape_text("a<b>&c"), "a&lt;b&gt;&amp;c");
    }

    #[test]
    fn span_style_emits_only_non_defaults() {
        let mut s = TextStyle::default();
        // Default style: black text, white bg, TNR 12 -> family+size only.
        let style = span_style(&s);
        assert!(style.contains("font-family:"));
        assert!(style.contains("font-size:12pt;"));
        assert!(!style.contains("color:"));
        assert!(!style.contains("background-color:"));
        s.bold = true;
        s.color = 0x00FF0000; // red-ish (non-default)
        let style = span_style(&s);
        assert!(style.contains("font-weight:bold;"));
        assert!(style.contains("color:#ff0000;"));
    }

    #[test]
    fn image_tag_strips_whitespace_and_detects_png() {
        let img = ImageRun {
            data: "iVBOR\n  w0KG\tgo".to_string(),
            width: 10.0,
            height: 20.0,
        };
        let tag = image_tag(&img);
        assert!(tag.contains("data:image/png;base64,iVBORw0KGgo"));
        assert!(tag.contains("width:10pt;"));
        assert!(tag.contains("height:20pt;"));
    }
}
