//! Parser for Tesseract's TSV output (`tesseract page.png out tsv`, one page or several). Tesseract
//! runs only locally, on scanned pages; this module reads its output and is pure.
//!
//! Columns: `level page_num block_num par_num line_num word_num left top width height conf text`;
//! level 5 rows are words, `conf` is 0-100 (-1 on structural rows). Each paragraph (level 3) becomes
//! one [`TextBlock`] of kind `text`, lines joined by `\n` and words by a space. OCR can misread digits:
//! the mean word confidence goes with the block so a caller can refuse to cite low-confidence numbers.
//! Tesseract finds no tables, so there are none here. hOCR is not parsed: the TSV carries the same words.

use crate::docparse::TextBlock;
use crate::{ParseError, ParseErrorKind};

pub const PARSER_VERSION: u32 = 1;

const HEADER: &str =
    "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext";

#[derive(Debug, Clone, PartialEq)]
pub struct OcrBlock {
    pub block: TextBlock,
    /// Mean of the words' `conf` (0-100).
    pub mean_conf: f32,
}

/// One paragraph being built: (page, block, paragraph) numbers, words per line, word confidences.
#[derive(Default)]
struct Para {
    key: (u32, u32, u32),
    lines: Vec<(u32, Vec<String>)>,
    confs: Vec<f32>,
}

pub fn parse_tsv(bytes: &[u8]) -> Result<Vec<OcrBlock>, ParseError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| ParseError::new(ParseErrorKind::Shape, "line 1", format!("not UTF-8: {e}")))?;
    let mut lines = text.lines();
    if lines.next().map(|l| l.trim_start_matches('\u{feff}')) != Some(HEADER) {
        return Err(ParseError::new(
            ParseErrorKind::Shape,
            "line 1",
            "not Tesseract TSV: header row differs",
        ));
    }
    let mut paras: Vec<Para> = Vec::new();
    for (n, line) in lines.enumerate() {
        let loc = format!("line {}", n + 2);
        if line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.splitn(12, '\t').collect();
        if f.len() < 11 {
            return Err(ParseError::new(
                ParseErrorKind::Shape,
                loc,
                "fewer than 11 columns",
            ));
        }
        let num = |i: usize| -> Result<u32, ParseError> {
            f[i].parse().map_err(|_| {
                ParseError::new(
                    ParseErrorKind::Value,
                    &loc,
                    format!("column {} is not a number", i + 1),
                )
            })
        };
        if num(0)? != 5 {
            continue;
        }
        let word = f.get(11).copied().unwrap_or("").trim();
        if word.is_empty() {
            continue;
        }
        let conf: f32 = f[10]
            .parse()
            .map_err(|_| ParseError::new(ParseErrorKind::Value, &loc, "conf is not a number"))?;
        let (key, line_no) = ((num(1)?, num(2)?, num(3)?), num(4)?);
        if paras.last().is_none_or(|p| p.key != key) {
            paras.push(Para::default());
            if let Some(p) = paras.last_mut() {
                p.key = key;
            }
        }
        let Some(p) = paras.last_mut() else {
            continue;
        };
        if p.lines.last().is_none_or(|l| l.0 != line_no) {
            p.lines.push((line_no, Vec::new()));
        }
        if let Some(l) = p.lines.last_mut() {
            l.1.push(word.to_owned());
        }
        p.confs.push(conf);
    }
    Ok(paras
        .into_iter()
        .enumerate()
        .map(|(i, p)| OcrBlock {
            block: TextBlock {
                block_no: u32::try_from(i).unwrap_or(u32::MAX),
                kind: Some("text".to_owned()),
                page_no: Some(p.key.0),
                text: p
                    .lines
                    .iter()
                    .map(|l| l.1.join(" "))
                    .collect::<Vec<_>>()
                    .join("\n"),
            },
            mean_conf: p.confs.iter().sum::<f32>() / p.confs.len() as f32,
        })
        .collect())
}
