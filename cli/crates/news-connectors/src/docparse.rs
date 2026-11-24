//! What the document parsers return: text blocks and tables, the shape Clerk stores in
//! `text_blocks` (`block_no`, `kind`, `page_no`, `text`) and cites with a PDF locator (Bureau §3.4).
//!
//! The parsers ([`crate::docling`], [`crate::tesseract`]) read the recorded output of tools that run
//! only locally (Docling in the single-slot `docling` pool). Like every parser here they are pure and
//! never convert or reformat: cell and word text is exactly what the tool returned.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// One row of `text_blocks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextBlock {
    pub block_no: u32,
    /// Docling's label (`section_header`, `text`, `list_item` …); `text` for OCR paragraphs.
    pub kind: Option<String>,
    pub page_no: Option<u32>,
    pub text: String,
}

/// One table cell, at its top-left grid position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub row: u32,
    pub col: u32,
    pub row_span: u32,
    pub col_span: u32,
    pub text: String,
    pub header: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    /// Position among the document's tables (`#/tables/<index>` in Docling).
    pub index: u32,
    pub page_no: Option<u32>,
    pub captions: Vec<String>,
    pub footnotes: Vec<String>,
    pub num_rows: u32,
    pub num_cols: u32,
    pub cells: Vec<Cell>,
}

impl Table {
    /// The PDF locator of a cell: page, table and grid position (ADR 0018).
    pub fn locator(&self, cell: &Cell) -> Value {
        json!({"kind": "pdf", "page": self.page_no, "table": self.index, "row": cell.row, "col": cell.col})
    }
}

/// Blocks and tables in reading order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub blocks: Vec<TextBlock>,
    pub tables: Vec<Table>,
}
