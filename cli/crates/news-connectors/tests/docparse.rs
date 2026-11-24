//! The Docling and Tesseract output parsers, from recorded-format outputs (no tools, no network).
//! See `tests/fixtures/README.md` for how the fixtures were made and how to re-record them.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use news_connectors::{ParseErrorKind, docling, tesseract};
use serde_json::{Value, json};

fn fixture(name: &str) -> Vec<u8> {
    let p = format!(
        "{}/tests/fixtures/docparse/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

#[test]
fn docling_blocks_come_out_in_reading_order_without_furniture_captions_or_pictures() {
    let doc = docling::parse(&fixture("docling-cpi-table.json")).unwrap();
    let got: Vec<_> = doc
        .blocks
        .iter()
        .map(|b| {
            (
                b.block_no,
                b.kind.as_deref().unwrap(),
                b.page_no.unwrap(),
                b.text.as_str(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (
                0,
                "section_header",
                1,
                "Table 1. Consumer Price Index for All Urban Consumers (CPI-U), U.S. city average"
            ),
            (
                1,
                "text",
                1,
                "Index levels, 1982-84=100, not seasonally adjusted. P = preliminary."
            ),
            (2, "section_header", 2, "Notes"),
            (3, "list_item", 2, "Data for October 2026 are preliminary."),
            (4, "list_item", 2, "Index values are shown as published."),
        ]
    );
}

#[test]
fn docling_table_keeps_cell_text_spans_headers_and_caption() {
    let doc = docling::parse(&fixture("docling-cpi-table.json")).unwrap();
    let [t] = doc.tables.as_slice() else {
        panic!("one table")
    };
    assert_eq!(
        (t.index, t.page_no, t.num_rows, t.num_cols),
        (0, Some(1), 7, 2)
    );
    assert_eq!(t.captions, ["Table 1. CPI-U, all items, 2026"]);
    assert_eq!(t.footnotes, ["P = preliminary."]);
    assert_eq!(t.cells.len(), 13);
    let at = |r, c| t.cells.iter().find(|x| x.row == r && x.col == c).unwrap();
    assert!(at(0, 1).header && !at(1, 1).header);
    // Values exactly as printed, including the footnote marker.
    assert_eq!(at(1, 1).text, "333.952");
    assert_eq!(at(5, 1).text, "337.511 (P)");
    let src = at(6, 0);
    assert_eq!((src.row_span, src.col_span), (1, 2));
    assert_eq!(
        t.locator(at(5, 1)),
        json!({"kind": "pdf", "page": 1, "table": 0, "row": 5, "col": 1})
    );
}

#[test]
fn docling_rejects_other_json_a_newer_major_and_a_cell_outside_the_grid() {
    let good: Value = serde_json::from_slice(&fixture("docling-cpi-table.json")).unwrap();
    let err = |v: &Value| docling::parse(v.to_string().as_bytes()).unwrap_err();

    assert_eq!(err(&json!({"a": 1})).kind, ParseErrorKind::Shape);
    let mut v = good.clone();
    v["version"] = json!("2.0.0");
    assert_eq!(err(&v).kind, ParseErrorKind::Shape);
    let mut v = good.clone();
    v["body"]["children"][0] = json!({"$ref": "#/texts/99"});
    let e = err(&v);
    assert_eq!(
        (e.kind, e.locator.as_str()),
        (ParseErrorKind::Shape, "#/texts/99")
    );
    let mut v = good.clone();
    v["tables"][0]["data"]["table_cells"][3]["end_col_offset_idx"] = json!(9);
    let e = err(&v);
    assert_eq!(e.kind, ParseErrorKind::Value);
    assert_eq!(e.locator, "#/tables/0/data/table_cells/3");
    assert_eq!(
        docling::parse(b"not json").unwrap_err().kind,
        ParseErrorKind::Shape
    );
}

#[test]
fn docling_group_cycle_is_an_error_not_a_hang() {
    let mut v: Value = serde_json::from_slice(&fixture("docling-cpi-table.json")).unwrap();
    v["groups"][0]["children"] = json!([{"$ref": "#/groups/0"}]);
    let e = docling::parse(v.to_string().as_bytes()).unwrap_err();
    assert_eq!(e.kind, ParseErrorKind::Shape);
}

#[test]
fn tesseract_paragraphs_join_words_and_lines_and_carry_confidence() {
    let blocks = tesseract::parse_tsv(&fixture("tesseract-cpi.tsv")).unwrap();
    let got: Vec<_> = blocks
        .iter()
        .map(|b| (b.block.block_no, b.block.page_no, b.block.text.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (
                0,
                Some(1),
                "Consumer Price Index\nOctober 2026: 337.511 (P)"
            ),
            (1, Some(1), "September 336.940"),
        ]
    );
    assert!(
        blocks
            .iter()
            .all(|b| b.block.kind.as_deref() == Some("text"))
    );
    // Mean of 96.512, 96.201, 95.874, 95.310, 94.775, 91.402, 63.118: the (P) is the weak word.
    assert!(
        (blocks[0].mean_conf - 90.456).abs() < 0.01,
        "{}",
        blocks[0].mean_conf
    );
    // The trailing word with empty text (conf -1) does not count.
    assert!((blocks[1].mean_conf - 94.83).abs() < 0.01);
}

#[test]
fn tesseract_rejects_other_text_and_bad_numbers() {
    let e = tesseract::parse_tsv(b"hello\tworld\n").unwrap_err();
    assert_eq!(e.kind, ParseErrorKind::Shape);
    let mut bad = String::from_utf8(fixture("tesseract-cpi.tsv")).unwrap();
    bad.push_str("5\t1\t2\t1\t1\t4\t0\t0\t1\t1\tx\tword\n");
    let e = tesseract::parse_tsv(bad.as_bytes()).unwrap_err();
    assert_eq!(e.kind, ParseErrorKind::Value);
    assert_eq!(e.locator, "line 20");
    assert!(tesseract::parse_tsv(b"").is_err());
}
