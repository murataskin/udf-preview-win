//! The shared document model, mirroring the reference app's `Model/` directory.
//!
//! This is the single intermediate representation the parser produces and both backends
//! (HTML, RTF) consume. Run text stored here is **already rune-sliced and cleaned** by the
//! parser — backends never see raw CDATA or character offsets.
//!
//! Field names are the Swift names in `snake_case`; defaults match the reference exactly,
//! because the parser uses these defaults as attribute fallbacks (e.g. a missing `family`
//! falls back to [`TextStyle::default`]'s `font_family`). Getting a default wrong silently
//! corrupts every document that omits that attribute.

use std::collections::BTreeMap;

/// A fully parsed UDF document (Swift `UDFDocument`).
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub pages: PageFormat,
    pub header: Vec<BlockElement>,
    pub body: Vec<BlockElement>,
    pub footer: Vec<BlockElement>,
    pub footer_page_number: Option<FooterPageNumber>,
    pub format_id: String,
    pub resolver: String,
    pub styles: Vec<DocumentStyle>,
}

impl Default for Document {
    fn default() -> Self {
        Document {
            pages: PageFormat::default(),
            header: Vec::new(),
            body: Vec::new(),
            footer: Vec::new(),
            footer_page_number: None,
            format_id: "1.8".to_string(),
            resolver: "hvl-default".to_string(),
            styles: DocumentStyle::defaults(),
        }
    }
}

/// A block-level element in document order.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockElement {
    Paragraph(Paragraph),
    Table(Table),
    PageBreak,
}

// ---------------------------------------------------------------------------
// Paragraph + inline runs
// ---------------------------------------------------------------------------

/// Horizontal paragraph alignment. Raw values: left=0, center=1, right=2, justify=3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

impl Alignment {
    /// Map the `Alignment` attribute string. Mirrors `parseAlignment`: "1"->center,
    /// "2"->right, "3"->justify; everything else (incl. "0", missing) -> left.
    pub fn from_attr(value: Option<&str>) -> Alignment {
        match value {
            Some("1") => Alignment::Center,
            Some("2") => Alignment::Right,
            Some("3") => Alignment::Justify,
            _ => Alignment::Left,
        }
    }

    /// The integer raw value UYAP writes.
    pub fn raw(self) -> i32 {
        match self {
            Alignment::Left => 0,
            Alignment::Center => 1,
            Alignment::Right => 2,
            Alignment::Justify => 3,
        }
    }
}

/// An inline run. Three cases — text, an embedded image, or a tab.
#[derive(Debug, Clone, PartialEq)]
pub enum InlineElement {
    Text(TextRun),
    Image(ImageRun),
    Tab(TabRun),
}

/// A styled run of text. `text` is already sliced + cleaned (zero-width spaces removed).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextRun {
    pub text: String,
    pub style: TextStyle,
}

/// A tab stop occurrence within a paragraph's run sequence.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TabRun {
    pub style: TextStyle,
}

/// An embedded image. `data` is the raw (possibly whitespace-laden) base64 string from the
/// UDF — backends must strip whitespace leniently before emitting it. Width/height in points.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImageRun {
    pub data: String,
    pub width: f64,
    pub height: f64,
}

/// List kind (Swift `ListType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListType {
    Numbered,
    Bulleted,
}

/// List membership/formatting for a paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct ListProperties {
    pub list_type: ListType,
    pub number_type: Option<String>,
    pub bullet_type: Option<String>,
    /// 0-based nesting depth.
    pub level: i32,
    pub list_id: Option<i32>,
}

/// A paragraph: a sequence of inline runs plus block formatting.
///
/// Note: alignment/indent/spacing live here on the paragraph, **not** on [`TextStyle`].
/// `line_spacing` is an offset from 1.0 (0.0 = single, 0.5 = 1.5x, 1.0 = double).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Paragraph {
    pub alignment: Alignment,
    pub left_indent: f64,
    pub right_indent: f64,
    pub first_line_indent: f64,
    pub line_spacing: f64,
    pub space_before: f64,
    pub space_after: f64,
    pub runs: Vec<InlineElement>,
    pub list: Option<ListProperties>,
    pub tab_stops: Vec<f64>,
    /// Font of the empty-paragraph placeholder, captured so a blank line round-trips with
    /// the same family/size the official editor wrote.
    pub empty_content_style: Option<TextStyle>,
}

// ---------------------------------------------------------------------------
// Text style
// ---------------------------------------------------------------------------

/// Character-level styling. Holds **no** alignment/indent (those are on [`Paragraph`]).
/// `color` and `background_color` are signed ARGB `i32`.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub font_family: String,
    pub font_size: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub color: i32,
    pub background_color: i32,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            font_family: "Times New Roman".to_string(),
            font_size: 12.0,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            color: -16777216,     // black, 0xFF000000
            background_color: -1, // white, 0xFFFFFFFF
        }
    }
}

// ---------------------------------------------------------------------------
// Tables (nested tables supported)
// ---------------------------------------------------------------------------

/// Border name/style tokens (Swift `BorderStyle`). Defaults: "borderCell" / "borderStyle-solid".
#[derive(Debug, Clone, PartialEq)]
pub struct BorderStyle {
    /// "borderCell", "borderTable", "borderNone".
    pub name: String,
    /// "borderStyle-solid", "borderStyle-none".
    pub style: String,
}

impl Default for BorderStyle {
    fn default() -> Self {
        BorderStyle {
            name: "borderCell".to_string(),
            style: "borderStyle-solid".to_string(),
        }
    }
}

/// Vertical alignment of cell content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalAlignment {
    #[default]
    Top,
    Middle,
    Bottom,
}

impl VerticalAlignment {
    /// Mirrors `parseVerticalAlign`.
    pub fn from_attr(value: Option<&str>) -> VerticalAlignment {
        match value {
            Some("vcenter") | Some("center") | Some("middle") => VerticalAlignment::Middle,
            Some("bottom") => VerticalAlignment::Bottom,
            _ => VerticalAlignment::Top,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub colspan: i32,
    pub rowspan: i32,
    pub vertical_align: VerticalAlignment,
    /// ARGB i32. Default 16777215 (white, unsigned).
    pub fill_color: i32,
    pub border: BorderStyle,
    pub border_width: f64,
    /// ARGB i32. As a **border** colour, `0` renders opaque black (see `color` module).
    pub border_color: i32,
    /// Bitmask: top=1, right=2, bottom=4, left=8.
    pub border_spec: i32,
    /// Relative width (from row/table columnSpans).
    pub width: Option<f64>,
    /// Cell content. Nested tables are supported via `BlockElement::Table`.
    pub content: Vec<BlockElement>,
}

impl Default for TableCell {
    fn default() -> Self {
        TableCell {
            colspan: 1,
            rowspan: 1,
            vertical_align: VerticalAlignment::Top,
            fill_color: 16777215,
            border: BorderStyle::default(),
            border_width: 0.5,
            border_color: 0,
            border_spec: 15,
            width: None,
            content: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableRow {
    pub name: String,
    pub row_type: String,
    /// Actual height (from table rowSpans).
    pub height: f64,
    /// User-set minimum (`height_min`).
    pub height_min: Option<f64>,
    pub border: BorderStyle,
    /// Row vertical-align token (tolerated; never emitted).
    pub valign: Option<String>,
    pub cells: Vec<TableCell>,
}

impl Default for TableRow {
    fn default() -> Self {
        TableRow {
            name: "row1".to_string(),
            row_type: "dataRow".to_string(),
            height: 0.0,
            height_min: None,
            border: BorderStyle::default(),
            valign: None,
            cells: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub name: String,
    pub columns: i32,
    pub column_widths: Vec<f64>,
    pub border: BorderStyle,
    pub rows: Vec<TableRow>,
    // Optional table-level attributes carried verbatim for fidelity.
    pub align: Option<String>,
    /// e.g. "16.00cm" — a string in the UDF, not a number.
    pub width: Option<String>,
    pub cellpadding: Option<f64>,
    pub cellspacing: Option<f64>,
    pub border_width: Option<f64>,
    pub border_color: Option<i32>,
    pub border_spec: Option<i32>,
}

impl Default for Table {
    fn default() -> Self {
        Table {
            name: "Sabit".to_string(),
            columns: 0,
            column_widths: Vec::new(),
            border: BorderStyle::default(),
            rows: Vec::new(),
            align: None,
            width: None,
            cellpadding: None,
            cellspacing: None,
            border_width: None,
            border_color: None,
            border_spec: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Page format
// ---------------------------------------------------------------------------

/// Page geometry from `<pageFormat>`. Raw fields mirror the reference exactly.
#[derive(Debug, Clone, PartialEq)]
pub struct PageFormat {
    /// 1 = A4.
    pub media_size_name: i32,
    pub left_margin: f64,
    pub right_margin: f64,
    pub top_margin: f64,
    pub bottom_margin: f64,
    /// 1 = portrait, 2 = landscape.
    pub paper_orientation: i32,
    pub header_f_offset: f64,
    pub footer_f_offset: f64,
}

impl Default for PageFormat {
    fn default() -> Self {
        PageFormat {
            media_size_name: 1,
            left_margin: 42.52,
            right_margin: 28.35,
            top_margin: 14.17,
            bottom_margin: 14.17,
            paper_orientation: 1,
            header_f_offset: 20.0,
            footer_f_offset: 20.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Footer page-number metadata + named styles
// ---------------------------------------------------------------------------

/// Metadata controlling the optional page-number glyph UYAP renders inside a footer.
/// Captured verbatim for round-tripping; values are opaque strings.
#[derive(Debug, Clone, PartialEq)]
pub struct FooterPageNumber {
    pub spec: String,
    pub font_face: String,
    pub font_size: f64,
    pub font_bold: bool,
    pub font_italic: bool,
    pub color: i32,
    pub fore_str: String,
    pub page_start_num_str: String,
    pub extra_attributes: BTreeMap<String, String>,
}

impl Default for FooterPageNumber {
    fn default() -> Self {
        FooterPageNumber {
            spec: String::new(),
            font_face: "Times New Roman".to_string(),
            font_size: 11.0,
            font_bold: false,
            font_italic: false,
            color: -16777216,
            fore_str: String::new(),
            page_start_num_str: String::new(),
            extra_attributes: BTreeMap::new(),
        }
    }
}

/// One non-`name` attribute of a `<style>`, preserved verbatim in document order.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleAttribute {
    pub name: String,
    pub value: String,
}

/// One `<style>` definition from a document's `<styles>` block.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocumentStyle {
    pub name: String,
    pub attributes: Vec<StyleAttribute>,
}

impl DocumentStyle {
    /// The two styles UYAP emits for a fresh document (matches legacy output).
    pub fn defaults() -> Vec<DocumentStyle> {
        let a = |name: &str, value: &str| StyleAttribute {
            name: name.to_string(),
            value: value.to_string(),
        };
        vec![
            DocumentStyle {
                name: "default".to_string(),
                attributes: vec![
                    a("description", "Geçerli"),
                    a("family", "Dialog"),
                    a("size", "12"),
                    a("bold", "false"),
                    a("italic", "false"),
                    a("foreground", "-13421773"),
                    a(
                        "FONT_ATTRIBUTE_KEY",
                        "javax.swing.plaf.FontUIResource[family=Dialog,name=Dialog,style=plain,size=12]",
                    ),
                ],
            },
            DocumentStyle {
                name: "hvl-default".to_string(),
                attributes: vec![
                    a("family", "Times New Roman"),
                    a("size", "12"),
                    a("description", "Gövde"),
                ],
            },
        ]
    }
}
