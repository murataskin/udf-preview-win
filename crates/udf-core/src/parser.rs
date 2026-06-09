//! Document parser — ports `UDFParser.swift`.
//!
//! Walks `<elements>` (header / body / footer in document order), rune-slices the CDATA per
//! each element's `startOffset` + `length`, and builds the [`Document`]. Note: run styling is
//! read directly from each run element's font attributes with [`TextStyle::default`]
//! fallbacks — the reference does **not** resolve named `<styles>` into runs (those are kept
//! only for round-trip fidelity).

use roxmltree::Node;

use crate::model::*;
use crate::rune::rune_substring_clamped;
use crate::xml::*;
use crate::UdfError;

/// Parse `.udf` bytes into a [`Document`].
pub fn parse(bytes: &[u8]) -> Result<Document, UdfError> {
    let xml_bytes = crate::zip::extract_content_xml(bytes)?;
    let xml = std::str::from_utf8(&xml_bytes)
        .map_err(|_| UdfError::InvalidXml("content.xml is not valid UTF-8".to_string()))?;

    let doc = roxmltree::Document::parse(xml).map_err(|e| UdfError::InvalidXml(e.to_string()))?;

    let template = doc.root_element();
    if template.tag_name().name() != "template" {
        return Err(UdfError::MalformedDocument(
            "missing <template> root element".to_string(),
        ));
    }

    let content_el = first_child_element(template, "content")
        .ok_or_else(|| UdfError::MalformedDocument("missing <content> element".to_string()))?;
    let cdata = node_text(content_el);

    let pages = first_child_element(template, "properties")
        .map(parse_page_format)
        .unwrap_or_default();

    let elements_el = first_child_element(template, "elements");
    let parsed = elements_el
        .map(|el| parse_elements(el, &cdata))
        .unwrap_or_default();

    let format_id = attr_str(template, "format_id", "1.8");
    let resolver = elements_el
        .map(|el| attr_str(el, "resolver", "hvl-default"))
        .unwrap_or_else(|| "hvl-default".to_string());
    let styles = parse_styles(template);

    Ok(Document {
        pages,
        header: parsed.header,
        body: parsed.body,
        footer: parsed.footer,
        footer_page_number: parsed.footer_page_number,
        format_id,
        resolver,
        styles,
    })
}

#[derive(Default)]
struct ParsedElements {
    header: Vec<BlockElement>,
    body: Vec<BlockElement>,
    footer: Vec<BlockElement>,
    footer_page_number: Option<FooterPageNumber>,
}

// MARK: - Styles

fn parse_styles(template: Node) -> Vec<DocumentStyle> {
    let Some(styles_el) = first_child_element(template, "styles") else {
        return DocumentStyle::defaults();
    };
    let mut out = Vec::new();
    for style_el in child_elements(styles_el, "style") {
        let name = attr_str(style_el, "name", "");
        let mut attributes = Vec::new();
        for attr in style_el.attributes() {
            if attr.name() == "name" {
                continue;
            }
            attributes.push(StyleAttribute {
                name: attr.name().to_string(),
                value: attr.value().to_string(),
            });
        }
        out.push(DocumentStyle { name, attributes });
    }
    if out.is_empty() {
        DocumentStyle::defaults()
    } else {
        out
    }
}

// MARK: - Page Format

fn parse_page_format(properties_el: Node) -> PageFormat {
    let Some(pf) = first_child_element(properties_el, "pageFormat") else {
        return PageFormat::default();
    };
    let d = PageFormat::default();
    PageFormat {
        media_size_name: attr_i32(pf, "mediaSizeName", 1),
        left_margin: attr_f64(pf, "leftMargin", d.left_margin),
        right_margin: attr_f64(pf, "rightMargin", d.right_margin),
        top_margin: attr_f64(pf, "topMargin", d.top_margin),
        bottom_margin: attr_f64(pf, "bottomMargin", d.bottom_margin),
        paper_orientation: attr_i32(pf, "paperOrientation", 1),
        header_f_offset: attr_f64(pf, "headerFOffset", d.header_f_offset),
        footer_f_offset: attr_f64(pf, "footerFOffset", d.footer_f_offset),
    }
}

// MARK: - Elements

fn parse_elements(elements_el: Node, cdata: &str) -> ParsedElements {
    let mut result = ParsedElements::default();
    for el in elements_el.children().filter(|n| n.is_element()) {
        match el.tag_name().name() {
            "paragraph" => result
                .body
                .push(BlockElement::Paragraph(parse_paragraph(el, cdata))),
            "table" => result
                .body
                .push(BlockElement::Table(parse_table(el, cdata))),
            "header" => result.header = parse_section_children(el, cdata),
            "footer" => {
                result.footer = parse_section_children(el, cdata);
                result.footer_page_number = parse_footer_page_number(el);
            }
            "page-break" => result.body.push(BlockElement::PageBreak),
            _ => {}
        }
    }
    result
}

fn parse_section_children(el: Node, cdata: &str) -> Vec<BlockElement> {
    let mut blocks = Vec::new();
    for child in el.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "paragraph" => blocks.push(BlockElement::Paragraph(parse_paragraph(child, cdata))),
            "table" => blocks.push(BlockElement::Table(parse_table(child, cdata))),
            _ => {}
        }
    }
    blocks
}

fn parse_footer_page_number(el: Node) -> Option<FooterPageNumber> {
    const KNOWN: [&str; 8] = [
        "pageNumber-spec",
        "pageNumber-fontFace",
        "pageNumber-fontSize",
        "pageNumber-fontBold",
        "pageNumber-fontItalic",
        "pageNumber-color",
        "pageNumber-foreStr",
        "pageNumber-pageStartNumStr",
    ];
    const BOUNDARY: [&str; 2] = ["color-boundary", "stroke-boundary"];

    let has_any_page_number = KNOWN.iter().any(|k| attr_present(el, k));
    let has_boundary = BOUNDARY.iter().any(|k| attr_present(el, k));
    if !has_any_page_number && !has_boundary {
        return None;
    }

    let mut extras = std::collections::BTreeMap::new();
    for attr in el.attributes() {
        if KNOWN.contains(&attr.name()) {
            continue;
        }
        extras.insert(attr.name().to_string(), attr.value().to_string());
    }

    Some(FooterPageNumber {
        spec: attr_str(el, "pageNumber-spec", ""),
        font_face: attr_str(el, "pageNumber-fontFace", "Times New Roman"),
        font_size: attr_f64(el, "pageNumber-fontSize", 11.0),
        font_bold: attr_bool(el, "pageNumber-fontBold"),
        font_italic: attr_bool(el, "pageNumber-fontItalic"),
        color: attr_i32(el, "pageNumber-color", -16777216),
        fore_str: attr_str(el, "pageNumber-foreStr", ""),
        page_start_num_str: attr_str(el, "pageNumber-pageStartNumStr", ""),
        extra_attributes: extras,
    })
}

// MARK: - Paragraph

fn parse_paragraph(el: Node, cdata: &str) -> Paragraph {
    let mut runs: Vec<InlineElement> = Vec::new();
    let mut empty_content_style: Option<TextStyle> = None;

    let has_list_attr = attr_present(el, "Bulleted") || attr_present(el, "Numbered");
    let element_children = element_child_count(el);

    for child in el.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "content" => {
                // Empty-paragraph placeholder: `<content startOffset="0" length="1">` that is
                // the paragraph's only child and not a list item. Capture its font so the
                // blank line round-trips, then stop processing this paragraph's children.
                let start_offset = attr_i64(child, "startOffset", -1);
                let length = attr_i64(child, "length", -1);
                let run = parse_text_run(child, cdata);
                let is_placeholder =
                    start_offset == 0 && length == 1 && element_children == 1 && !has_list_attr;
                if is_placeholder || run.text.is_empty() {
                    if empty_content_style.is_none() {
                        empty_content_style = Some(run.style);
                    }
                    break;
                }
                runs.push(InlineElement::Text(run));
            }
            "image" => runs.push(InlineElement::Image(parse_image_run(child))),
            "tab" => runs.push(InlineElement::Tab(TabRun {
                style: parse_font_style(child),
            })),
            _ => {}
        }
    }

    // Drop text runs that became empty after zero-width-space removal.
    runs.retain(|e| !matches!(e, InlineElement::Text(r) if r.text.is_empty()));

    let mut paragraph = Paragraph {
        alignment: Alignment::from_attr(attr_opt(el, "Alignment")),
        left_indent: attr_f64(el, "LeftIndent", 0.0),
        right_indent: attr_f64(el, "RightIndent", 0.0),
        first_line_indent: attr_f64(el, "FirstLineIndent", 0.0),
        line_spacing: attr_f64(el, "LineSpacing", 0.0),
        space_before: attr_f64(el, "SpaceAbove", 0.0),
        space_after: attr_f64(el, "SpaceBelow", 0.0),
        runs,
        list: None,
        tab_stops: Vec::new(),
        empty_content_style: None,
    };

    if let Some(tab_set) = attr_opt(el, "TabSet").filter(|s| !s.is_empty()) {
        paragraph.tab_stops = tab_set
            .split([',', ';'])
            .filter_map(|part| part.split(':').next().and_then(|n| n.parse::<f64>().ok()))
            .collect();
    }

    let is_numbered = attr_bool(el, "Numbered");
    let is_bulleted = attr_bool(el, "Bulleted");
    if is_numbered || is_bulleted {
        let list_id = attr_opt(el, "ListId")
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse::<i32>().ok());
        paragraph.list = Some(ListProperties {
            list_type: if is_numbered {
                ListType::Numbered
            } else {
                ListType::Bulleted
            },
            number_type: attr_str_opt(el, "NumberType"),
            bullet_type: attr_str_opt(el, "BulletType"),
            level: attr_i32(el, "ListLevel", 0),
            list_id,
        });
    }

    if paragraph.runs.is_empty() {
        paragraph.empty_content_style = empty_content_style;
    }

    paragraph
}

// MARK: - Inline elements

fn parse_text_run(el: Node, cdata: &str) -> TextRun {
    let start_offset = attr_i64(el, "startOffset", 0);
    let length = attr_i64(el, "length", 0);
    let mut text = rune_substring_clamped(cdata, start_offset, length);
    // Zero-width space placeholder for empty paragraphs.
    text = text.replace('\u{200B}', "");
    TextRun {
        text,
        style: parse_font_style(el),
    }
}

fn parse_font_style(el: Node) -> TextStyle {
    let d = TextStyle::default();
    TextStyle {
        font_family: attr_str(el, "family", &d.font_family),
        font_size: attr_f64(el, "size", d.font_size),
        bold: attr_bool(el, "bold"),
        italic: attr_bool(el, "italic"),
        underline: attr_bool(el, "underline"),
        strikethrough: attr_bool(el, "strikethrough"),
        color: attr_i32(el, "foreground", d.color),
        background_color: attr_i32(el, "background", d.background_color),
    }
}

fn parse_image_run(el: Node) -> ImageRun {
    ImageRun {
        data: attr_str(el, "imageData", ""),
        width: attr_f64(el, "width", 0.0),
        height: attr_f64(el, "height", 0.0),
    }
}

// MARK: - Table

#[derive(Default, Clone)]
struct InheritedBorder {
    name: Option<String>,
    style: Option<String>,
    width: Option<f64>,
    color: Option<i32>,
    spec: Option<i32>,
}

fn parse_table(el: Node, cdata: &str) -> Table {
    let inherited = InheritedBorder {
        name: attr_str_opt(el, "border"),
        style: attr_str_opt(el, "borderStyle"),
        width: attr_f64_opt(el, "borderWidth"),
        color: attr_i32_opt(el, "borderColor"),
        spec: attr_i32_opt(el, "borderSpec"),
    };
    let column_count = attr_i32(el, "columnCount", 0);
    let mut default_widths: Vec<f64> = parse_csv_f64(&attr_str(el, "columnSpans", ""));
    let cols = (column_count as usize).max(default_widths.len());
    while default_widths.len() < cols {
        default_widths.push(100.0);
    }
    let row_heights = parse_csv_f64(&attr_str(el, "rowSpans", ""));

    let mut rows = Vec::new();
    for (i, row_el) in child_elements(el, "row").enumerate() {
        let mut row = parse_table_row(row_el, cdata, &inherited, &default_widths);
        if i < row_heights.len() {
            row.height = row_heights[i];
        }
        rows.push(row);
    }

    Table {
        name: attr_str(el, "tableName", "Sabit"),
        columns: cols.max(1) as i32,
        column_widths: default_widths,
        border: parse_border_style(el),
        rows,
        align: attr_str_opt(el, "align"),
        width: attr_str_opt(el, "width"),
        cellpadding: attr_f64_opt(el, "cellpadding"),
        cellspacing: attr_f64_opt(el, "cellspacing"),
        border_width: inherited.width,
        border_color: inherited.color,
        border_spec: inherited.spec,
    }
}

fn parse_table_row(
    el: Node,
    cdata: &str,
    inherited: &InheritedBorder,
    default_widths: &[f64],
) -> TableRow {
    let row_widths = parse_csv_f64(&attr_str(el, "columnSpans", ""));
    let mut cells = Vec::new();
    for (i, cell_el) in child_elements(el, "cell").enumerate() {
        let mut cell = parse_table_cell(cell_el, cdata, inherited);
        let w = if !row_widths.is_empty() {
            *row_widths.get(i).unwrap_or(&100.0)
        } else {
            *default_widths.get(i).unwrap_or(&100.0)
        };
        if cell.width.is_none() {
            cell.width = Some(w);
        }
        cells.push(cell);
    }
    TableRow {
        name: attr_str(el, "rowName", "row1"),
        row_type: attr_str(el, "rowType", "dataRow"),
        height: attr_f64(el, "height", 0.0),
        height_min: attr_f64_opt(el, "height_min"),
        border: parse_border_style(el),
        valign: attr_str_opt(el, "valign"),
        cells,
    }
}

fn parse_table_cell(el: Node, cdata: &str, inherited: &InheritedBorder) -> TableCell {
    let mut content = Vec::new();
    for child in el.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "paragraph" => content.push(BlockElement::Paragraph(parse_paragraph(child, cdata))),
            "table" => content.push(BlockElement::Table(parse_table(child, cdata))),
            _ => {}
        }
    }

    let d = BorderStyle::default();
    let border_name = attr_str_opt(el, "border")
        .or_else(|| inherited.name.clone())
        .unwrap_or(d.name);
    let border_style = attr_str_opt(el, "borderStyle")
        .or_else(|| inherited.style.clone())
        .unwrap_or(d.style);
    let border_width = attr_f64_opt(el, "borderWidth")
        .or(inherited.width)
        .unwrap_or(0.5);
    let border_color = attr_i32_opt(el, "borderColor")
        .or(inherited.color)
        .unwrap_or(0);
    let border_spec = attr_i32_opt(el, "borderSpec")
        .or(inherited.spec)
        .unwrap_or(15);

    let mut cell = TableCell {
        colspan: attr_i32(el, "colspan", 1),
        rowspan: attr_i32(el, "rowspan", 1),
        vertical_align: VerticalAlignment::from_attr(attr_opt(el, "align")),
        fill_color: attr_i32(el, "fillColor", 16777215),
        border: BorderStyle {
            name: border_name,
            style: border_style,
        },
        border_width,
        border_color,
        border_spec,
        width: None,
        content,
    };
    if cell.colspan > 1 {
        cell.width = Some(cell.colspan as f64 * 100.0);
        cell.colspan = 1;
    }
    cell
}

// MARK: - Helpers

fn parse_border_style(el: Node) -> BorderStyle {
    let d = BorderStyle::default();
    BorderStyle {
        name: attr_str(el, "border", &d.name),
        style: attr_str(el, "borderStyle", &d.style),
    }
}

/// Split a comma-separated list of floats, trimming whitespace and dropping invalid items.
fn parse_csv_f64(s: &str) -> Vec<f64> {
    s.split(',')
        .filter_map(|p| {
            let t = p.trim();
            if t.is_empty() {
                None
            } else {
                t.parse::<f64>().ok()
            }
        })
        .collect()
}
