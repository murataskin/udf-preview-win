//! RTF backend (lean, first-page) — feeds the off-screen RichEdit control that produces the
//! Explorer thumbnail in Phase 2. Low fidelity is acceptable: nested tables are flattened and
//! line-spacing is dropped. The output is a single brace-balanced RTF document.
//!
//! Mapping (mirrors the same model semantics as the HTML backend):
//!   * runs -> `{\fN\fsN\b\i\ul\strike\cfN\highlightN text}` (size in half-points)
//!   * paragraphs -> `\pard \ql/\qc/\qr/\qj \sbN \saN \fiN \liN \riN ... \par` (twips)
//!   * tabs -> `\tab`; images -> `{\pict\pngblip|\jpegblip ...hex...}`
//!   * tables -> `\trowd \cellxN ... \cell \row` with per-cell borders (borderColor=0 opaque)
//!
//! Units: points -> twips (×20); font size -> half-points (×2).

use base64::{engine::general_purpose::STANDARD, Engine};

use crate::color::{argb_to_rgba, border_rgb_opaque};
use crate::model::*;

/// Total content width used to lay out table columns, in twips (~ A4 minus margins).
const TABLE_WIDTH_TWIPS: f64 = 9000.0;

/// Render a [`Document`] to an RTF string.
pub fn to_rtf(doc: &Document) -> String {
    let mut b = RtfBuilder::new();
    b.blocks(&doc.header);
    b.blocks(&doc.body);
    b.blocks(&doc.footer);
    b.finish()
}

struct RtfBuilder {
    /// Body content, emitted first; the font/colour tables are prepended in [`finish`].
    body: String,
    fonts: Vec<String>,
    /// Colour palette (1-based in `\cfN`; colortbl slot 0 is the auto entry).
    colors: Vec<(u8, u8, u8)>,
    list_counters: std::collections::HashMap<Option<i32>, usize>,
}

impl RtfBuilder {
    fn new() -> Self {
        RtfBuilder {
            body: String::new(),
            fonts: vec!["Times New Roman".to_string()],
            colors: Vec::new(),
            list_counters: std::collections::HashMap::new(),
        }
    }

    fn font_idx(&mut self, family: &str) -> usize {
        let fam = if family.is_empty() {
            "Times New Roman"
        } else {
            family
        };
        if let Some(i) = self.fonts.iter().position(|f| f == fam) {
            return i;
        }
        self.fonts.push(fam.to_string());
        self.fonts.len() - 1
    }

    /// Register an ARGB colour, returning its 1-based `\cfN` index.
    fn color_idx(&mut self, argb: i32) -> usize {
        let (r, g, b, _) = argb_to_rgba(argb);
        self.color_rgb(r, g, b)
    }

    fn color_rgb(&mut self, r: u8, g: u8, b: u8) -> usize {
        if let Some(i) = self.colors.iter().position(|c| *c == (r, g, b)) {
            return i + 1;
        }
        self.colors.push((r, g, b));
        self.colors.len()
    }

    fn blocks(&mut self, blocks: &[BlockElement]) {
        for block in blocks {
            match block {
                BlockElement::Paragraph(p) => self.paragraph(p, ""),
                BlockElement::Table(t) => self.table(t),
                BlockElement::PageBreak => {
                    self.body
                        .push_str("\\pard\\qc ──────── Sayfa Sonu ────────\\par\n");
                }
            }
        }
    }

    fn paragraph(&mut self, p: &Paragraph, extra_prefix: &str) {
        self.body.push_str("\\pard");
        self.body.push_str(extra_prefix);
        self.body.push_str(match p.alignment {
            Alignment::Left => "\\ql",
            Alignment::Center => "\\qc",
            Alignment::Right => "\\qr",
            Alignment::Justify => "\\qj",
        });
        if p.space_before > 0.0 {
            self.body
                .push_str(&format!("\\sb{}", twips(p.space_before)));
        }
        if p.space_after > 0.0 {
            self.body.push_str(&format!("\\sa{}", twips(p.space_after)));
        }
        if p.left_indent != 0.0 {
            self.body.push_str(&format!("\\li{}", twips(p.left_indent)));
        }
        if p.right_indent != 0.0 {
            self.body
                .push_str(&format!("\\ri{}", twips(p.right_indent)));
        }
        if p.first_line_indent != 0.0 {
            self.body
                .push_str(&format!("\\fi{}", twips(p.first_line_indent)));
        }

        // List marker injected as text (lean parity with the HTML backend).
        if let Some(list) = &p.list {
            let marker = self.list_marker(list);
            self.body.push(' ');
            self.body.push_str(&escape_rtf(&marker));
            self.body.push_str("\\tab ");
        } else {
            self.body.push(' ');
        }

        for run in &p.runs {
            self.run(run);
        }
        self.body.push_str("\\par\n");
    }

    fn run(&mut self, run: &InlineElement) {
        match run {
            InlineElement::Text(t) => self.text_run(&t.text, &t.style),
            InlineElement::Tab(_) => self.body.push_str("\\tab "),
            InlineElement::Image(img) => self.image(img),
        }
    }

    fn text_run(&mut self, text: &str, style: &TextStyle) {
        if text.is_empty() {
            return;
        }
        let f = self.font_idx(&style.font_family);
        let cf = self.color_idx(style.color);
        let mut group = format!("{{\\f{f}\\fs{}", half_points(style.font_size));
        group.push_str(&format!("\\cf{cf}"));
        if style.bold {
            group.push_str("\\b");
        }
        if style.italic {
            group.push_str("\\i");
        }
        if style.underline {
            group.push_str("\\ul");
        }
        if style.strikethrough {
            group.push_str("\\strike");
        }
        if style.background_color != -1 {
            let hl = self.color_idx(style.background_color);
            group.push_str(&format!("\\highlight{hl}"));
        }
        group.push(' ');
        self.body.push_str(&group);
        self.body.push_str(&escape_rtf(text));
        self.body.push('}');
    }

    fn image(&mut self, img: &ImageRun) {
        let cleaned: String = img.data.chars().filter(|c| !c.is_whitespace()).collect();
        let Ok(bytes) = STANDARD.decode(cleaned.as_bytes()) else {
            return; // lenient: skip undecodable image rather than corrupt the RTF
        };
        if bytes.is_empty() {
            return;
        }
        let blip = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            "\\jpegblip"
        } else {
            "\\pngblip" // PNG and the safe default
        };
        let w = twips(img.width);
        let h = twips(img.height);
        self.body
            .push_str(&format!("{{\\pict{blip}\\picwgoal{w}\\pichgoal{h} "));
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            hex.push_str(&format!("{byte:02x}"));
        }
        self.body.push_str(&hex);
        self.body.push('}');
    }

    fn table(&mut self, t: &Table) {
        for row in &t.rows {
            let total: f64 = row
                .cells
                .iter()
                .map(|c| c.width.unwrap_or(100.0))
                .sum::<f64>()
                .max(1.0);

            // Row definition: borders + right cell boundaries.
            self.body.push_str("\\trowd\\trgaph108");
            let mut acc = 0.0;
            for cell in &row.cells {
                let defs = self.cell_border_defs(cell);
                self.body.push_str(&defs);
                acc += cell.width.unwrap_or(100.0);
                let cellx = (acc / total * TABLE_WIDTH_TWIPS).round() as i64;
                self.body.push_str(&format!("\\cellx{cellx}"));
            }

            // Cell contents.
            for cell in &row.cells {
                if cell.content.is_empty() {
                    self.body.push_str("\\pard\\intbl \\cell");
                    continue;
                }
                for block in &cell.content {
                    match block {
                        BlockElement::Paragraph(p) => self.cell_paragraph(p),
                        // Nested table: lean flatten — render its cells' paragraphs inline.
                        BlockElement::Table(inner) => self.flatten_table_into_cell(inner),
                        BlockElement::PageBreak => {}
                    }
                }
                self.body.push_str("\\cell");
            }
            self.body.push_str("\\row\n");
        }
        self.body.push_str("\\pard\n");
    }

    fn cell_paragraph(&mut self, p: &Paragraph) {
        self.body.push_str("\\pard\\intbl");
        self.body.push_str(match p.alignment {
            Alignment::Left => "\\ql",
            Alignment::Center => "\\qc",
            Alignment::Right => "\\qr",
            Alignment::Justify => "\\qj",
        });
        self.body.push(' ');
        for run in &p.runs {
            self.run(run);
        }
        // No \par before \cell — the cell terminator ends the paragraph.
    }

    fn flatten_table_into_cell(&mut self, t: &Table) {
        for row in &t.rows {
            for cell in &row.cells {
                for block in &cell.content {
                    if let BlockElement::Paragraph(p) = block {
                        self.body.push_str("\\pard\\intbl ");
                        for run in &p.runs {
                            self.run(run);
                        }
                    }
                }
            }
        }
    }

    /// Per-cell border control words for the four edges present in `border_spec`.
    fn cell_border_defs(&mut self, cell: &TableCell) -> String {
        if cell.border.style == "borderStyle-none" {
            return String::new();
        }
        let (r, g, b, _) = border_rgb_opaque(cell.border_color);
        let cf = self.color_rgb(r, g, b);
        let w = (cell.border_width * 20.0).round().max(1.0) as i64;
        let mut s = String::new();
        let mut edge = |bit: i32, kw: &str| {
            if cell.border_spec & bit != 0 {
                s.push_str(&format!("{kw}\\brdrs\\brdrw{w}\\brdrcf{cf}"));
            }
        };
        edge(1, "\\clbrdrt");
        edge(2, "\\clbrdrr");
        edge(4, "\\clbrdrb");
        edge(8, "\\clbrdrl");
        s
    }

    fn list_marker(&mut self, list: &ListProperties) -> String {
        match list.list_type {
            ListType::Bulleted => "\u{2022}".to_string(),
            ListType::Numbered => {
                let c = self.list_counters.entry(list.list_id).or_insert(0);
                *c += 1;
                format!("{}.", *c)
            }
        }
    }

    fn finish(self) -> String {
        let mut out = String::from("{\\rtf1\\ansi\\ansicpg1254\\deff0");

        // Font table.
        out.push_str("{\\fonttbl");
        for (i, fam) in self.fonts.iter().enumerate() {
            out.push_str(&format!("{{\\f{i}\\fnil {};}}", escape_rtf(fam)));
        }
        out.push('}');

        // Colour table: slot 0 = auto (empty), then the collected palette.
        out.push_str("{\\colortbl;");
        for (r, g, b) in &self.colors {
            out.push_str(&format!("\\red{r}\\green{g}\\blue{b};"));
        }
        out.push('}');

        out.push_str("\\f0\\fs24\n");
        out.push_str(&self.body);
        out.push('}');
        out
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn twips(points: f64) -> i64 {
    (points * 20.0).round() as i64
}

fn half_points(points: f64) -> i64 {
    let hp = (points * 2.0).round() as i64;
    if hp <= 0 {
        24 // default 12pt
    } else {
        hp
    }
}

/// Escape text for RTF: backslash/braces, and non-ASCII as `\uN?` (UTF-16 code units).
fn escape_rtf(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\n' => out.push_str("\\par\n"),
            '\t' => out.push_str("\\tab "),
            c if (c as u32) < 0x80 => out.push(c),
            c => {
                // Emit each UTF-16 code unit as a signed 16-bit \uN? escape.
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{}?", *unit as i16));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with(body: Vec<BlockElement>) -> Document {
        Document {
            body,
            ..Document::default()
        }
    }

    fn para(text: &str) -> BlockElement {
        BlockElement::Paragraph(Paragraph {
            runs: vec![InlineElement::Text(TextRun {
                text: text.to_string(),
                style: TextStyle::default(),
            })],
            ..Paragraph::default()
        })
    }

    fn braces_balanced(s: &str) -> bool {
        let mut depth = 0i32;
        let mut prev_bs = false;
        for c in s.chars() {
            match c {
                '{' if !prev_bs => depth += 1,
                '}' if !prev_bs => {
                    depth -= 1;
                    if depth < 0 {
                        return false;
                    }
                }
                _ => {}
            }
            prev_bs = c == '\\' && !prev_bs;
        }
        depth == 0
    }

    #[test]
    fn header_and_balance() {
        let rtf = to_rtf(&doc_with(vec![para("Hello")]));
        assert!(rtf.starts_with("{\\rtf1\\ansi"));
        assert!(rtf.contains("\\fonttbl"));
        assert!(rtf.contains("Hello"));
        assert!(braces_balanced(&rtf));
    }

    #[test]
    fn non_ascii_is_unicode_escaped() {
        // 'ş' (U+015F) -> \u351?
        let rtf = to_rtf(&doc_with(vec![para("şç")]));
        assert!(rtf.contains("\\u351?"));
        assert!(braces_balanced(&rtf));
    }

    #[test]
    fn braces_in_text_are_escaped() {
        let rtf = to_rtf(&doc_with(vec![para("a{b}c")]));
        assert!(rtf.contains("a\\{b\\}c"));
        assert!(braces_balanced(&rtf));
    }

    #[test]
    fn bold_run_emits_b() {
        let style = TextStyle {
            bold: true,
            ..TextStyle::default()
        };
        let doc = doc_with(vec![BlockElement::Paragraph(Paragraph {
            runs: vec![InlineElement::Text(TextRun {
                text: "x".to_string(),
                style,
            })],
            ..Paragraph::default()
        })]);
        let rtf = to_rtf(&doc);
        assert!(rtf.contains("\\b"));
        assert!(braces_balanced(&rtf));
    }

    #[test]
    fn table_emits_row_structure() {
        let cell = TableCell {
            content: vec![para("c")],
            ..TableCell::default()
        };
        let table = Table {
            columns: 1,
            column_widths: vec![100.0],
            rows: vec![TableRow {
                cells: vec![cell],
                ..TableRow::default()
            }],
            ..Table::default()
        };
        let rtf = to_rtf(&doc_with(vec![BlockElement::Table(table)]));
        assert!(rtf.contains("\\trowd"));
        assert!(rtf.contains("\\cell"));
        assert!(rtf.contains("\\row"));
        assert!(braces_balanced(&rtf));
    }
}
