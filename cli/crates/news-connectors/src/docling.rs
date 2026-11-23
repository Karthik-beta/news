//! Parser for Docling's JSON export (`DoclingDocument`, schema 1.x). Docling itself runs only locally,
//! in a container with `--network none`, in the single-slot `docling` pool (`doc.parse_docling`); this
//! module only reads its output, so it is pure and re-runnable on stored output.
//!
//! The body tree is walked in reading order (`body.children`, through `groups`). Page headers and
//! footers live in `furniture` and are not visited; pictures carry no text and are skipped. A table's
//! captions and footnotes belong to the table, not to the text blocks. Cell text is kept verbatim.

use std::collections::HashSet;

use serde::Deserialize;

use crate::docparse::{Cell, Document, Table, TextBlock};
use crate::{ParseError, ParseErrorKind};

pub const PARSER_VERSION: u32 = 1;

/// Deepest group nesting followed (guards against a reference cycle in a damaged file).
const MAX_DEPTH: usize = 32;

#[derive(Deserialize)]
struct Ref {
    #[serde(rename = "$ref")]
    target: String,
}

#[derive(Deserialize)]
struct Node {
    #[serde(default)]
    children: Vec<Ref>,
}

#[derive(Deserialize)]
struct Prov {
    page_no: u32,
}

#[derive(Deserialize)]
struct Text {
    label: String,
    text: String,
    #[serde(default)]
    prov: Vec<Prov>,
}

#[derive(Deserialize)]
struct RawCell {
    text: String,
    start_row_offset_idx: u32,
    end_row_offset_idx: u32,
    start_col_offset_idx: u32,
    end_col_offset_idx: u32,
    #[serde(default)]
    column_header: bool,
}

#[derive(Deserialize)]
struct TableData {
    num_rows: u32,
    num_cols: u32,
    table_cells: Vec<RawCell>,
}

#[derive(Deserialize)]
struct RawTable {
    #[serde(default)]
    prov: Vec<Prov>,
    #[serde(default)]
    captions: Vec<Ref>,
    #[serde(default)]
    footnotes: Vec<Ref>,
    data: TableData,
}

#[derive(Deserialize)]
struct Doc {
    schema_name: String,
    version: String,
    body: Node,
    #[serde(default)]
    groups: Vec<Node>,
    #[serde(default)]
    texts: Vec<Text>,
    #[serde(default)]
    tables: Vec<RawTable>,
}

fn shape(loc: &str, msg: impl Into<String>) -> ParseError {
    ParseError::new(ParseErrorKind::Shape, loc, msg)
}

/// Index of `#/<kind>/<n>`.
fn index(r: &Ref, kind: &str) -> Option<usize> {
    r.target
        .strip_prefix("#/")?
        .strip_prefix(kind)?
        .strip_prefix('/')?
        .parse()
        .ok()
}

fn text_of<'a>(doc: &'a Doc, r: &Ref) -> Result<&'a Text, ParseError> {
    index(r, "texts")
        .and_then(|i| doc.texts.get(i))
        .ok_or_else(|| shape(&r.target, "reference to a missing text item"))
}

pub fn parse(bytes: &[u8]) -> Result<Document, ParseError> {
    let doc: Doc = serde_json::from_slice(bytes)
        .map_err(|e| shape("$", format!("not a DoclingDocument: {e}")))?;
    if doc.schema_name != "DoclingDocument" || !doc.version.starts_with("1.") {
        return Err(shape(
            "$.schema_name",
            format!(
                "expected DoclingDocument 1.x, got {} {}",
                doc.schema_name, doc.version
            ),
        ));
    }
    let mut out = Document::default();
    // Captions and footnotes are claimed by their table, so they are not repeated as blocks.
    let mut claimed = HashSet::new();
    for t in &doc.tables {
        claimed.extend(
            t.captions
                .iter()
                .chain(&t.footnotes)
                .map(|r| r.target.as_str()),
        );
    }
    walk(&doc, &doc.body.children, &claimed, 0, &mut out)?;
    Ok(out)
}

fn walk(
    doc: &Doc,
    children: &[Ref],
    claimed: &HashSet<&str>,
    depth: usize,
    out: &mut Document,
) -> Result<(), ParseError> {
    if depth > MAX_DEPTH {
        return Err(shape(
            "$.body",
            "groups nested too deeply (a reference cycle?)",
        ));
    }
    for r in children {
        if let Some(i) = index(r, "groups") {
            let g = doc
                .groups
                .get(i)
                .ok_or_else(|| shape(&r.target, "reference to a missing group"))?;
            walk(doc, &g.children, claimed, depth + 1, out)?;
        } else if index(r, "texts").is_some() {
            let t = text_of(doc, r)?;
            if !claimed.contains(r.target.as_str()) && !t.text.trim().is_empty() {
                out.blocks.push(TextBlock {
                    block_no: u32::try_from(out.blocks.len()).unwrap_or(u32::MAX),
                    kind: Some(t.label.clone()),
                    page_no: t.prov.first().map(|p| p.page_no),
                    text: t.text.clone(),
                });
            }
        } else if let Some(i) = index(r, "tables") {
            let t = doc
                .tables
                .get(i)
                .ok_or_else(|| shape(&r.target, "reference to a missing table"))?;
            out.tables.push(table(doc, i, t)?);
        } else if index(r, "pictures").is_none() {
            return Err(shape(&r.target, "unknown reference kind"));
        }
    }
    Ok(())
}

fn table(doc: &Doc, index: usize, t: &RawTable) -> Result<Table, ParseError> {
    let loc = format!("#/tables/{index}");
    let words = |refs: &[Ref]| -> Result<Vec<String>, ParseError> {
        refs.iter()
            .map(|r| text_of(doc, r).map(|t| t.text.clone()))
            .collect()
    };
    let d = &t.data;
    let mut cells = Vec::with_capacity(d.table_cells.len());
    for (n, c) in d.table_cells.iter().enumerate() {
        if c.end_row_offset_idx <= c.start_row_offset_idx
            || c.end_col_offset_idx <= c.start_col_offset_idx
            || c.end_row_offset_idx > d.num_rows
            || c.end_col_offset_idx > d.num_cols
        {
            return Err(ParseError::new(
                ParseErrorKind::Value,
                format!("{loc}/data/table_cells/{n}"),
                "cell outside the table grid",
            ));
        }
        cells.push(Cell {
            row: c.start_row_offset_idx,
            col: c.start_col_offset_idx,
            row_span: c.end_row_offset_idx - c.start_row_offset_idx,
            col_span: c.end_col_offset_idx - c.start_col_offset_idx,
            text: c.text.clone(),
            header: c.column_header,
        });
    }
    Ok(Table {
        index: u32::try_from(index).unwrap_or(u32::MAX),
        page_no: t.prov.first().map(|p| p.page_no),
        captions: words(&t.captions)?,
        footnotes: words(&t.footnotes)?,
        num_rows: d.num_rows,
        num_cols: d.num_cols,
        cells,
    })
}
