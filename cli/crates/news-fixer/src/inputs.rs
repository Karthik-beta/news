//! What Fixer reads for one story, loaded from PostgreSQL by [`crate::db::load`] or built from
//! recorded fixtures in tests. Values are the Ledger's verbatim text; nothing here is converted.

use chrono::{DateTime, NaiveDate, Utc};
use news_core::brief::{Action, Entity, RelatedPage};
use news_ledger::PeriodType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Inputs {
    pub story_id: i64,
    pub event_key: Option<String>,
    /// The page-type skill's name without `page-` ([`crate::prefix::skill_page_type`]).
    pub page_type: String,
    pub tier: u8,
    pub action: Action,
    /// Taxonomy slugs of the best candidate, parent first.
    pub categories: Vec<String>,
    pub entities: Vec<Entity>,
    /// The story's own data points (its candidates' `dp_ids`), the best candidate's first.
    pub points: Vec<Point>,
    /// Earlier points of the same series, as of the story's data (latest vintage per period). May
    /// repeat story points.
    pub history: Vec<Point>,
    /// The story's documents, the best candidate's first.
    pub documents: Vec<Document>,
    /// Images the sources published, as the connectors recorded them (URL, credit, licence, alt).
    pub images: Vec<serde_json::Value>,
    pub related: Vec<RelatedPage>,
}

/// One `data_points` row, the columns Fixer uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub dp_id: String,
    pub series_key: String,
    pub label: String,
    pub value_text: String,
    pub unit: String,
    pub currency: Option<String>,
    pub scale: Option<String>,
    pub seasonal_adj: Option<String>,
    pub period_label: String,
    pub period_start: Option<NaiveDate>,
    pub period_end: Option<NaiveDate>,
    pub period_type: PeriodType,
    pub fiscal_year: Option<i16>,
    pub fiscal_period: Option<String>,
    pub vintage: DateTime<Utc>,
}

impl Point {
    /// The Ledger's form, for its derived-value guards. Provenance fields the guards never read are
    /// left empty.
    pub fn ledger(&self) -> Option<news_ledger::DataPoint> {
        let (value, style) = news_ledger::decimal::parse(&self.value_text).ok()?;
        Some(news_ledger::DataPoint {
            dp_id: self.dp_id.clone(),
            context_key: String::new(),
            series_key: self.series_key.clone(),
            entity: None,
            concept: String::new(),
            label: self.label.clone(),
            value_text: self.value_text.clone(),
            value: Some(value),
            style,
            unit: self.unit.clone(),
            currency: self.currency.clone(),
            scale: self.scale.clone(),
            decimals: None,
            period_label: self.period_label.clone(),
            period_start: self.period_start,
            period_end: self.period_end,
            period_type: self.period_type,
            fiscal_year: self.fiscal_year,
            fiscal_period: self.fiscal_period.clone(),
            seasonal_adj: self.seasonal_adj.clone(),
            vintage: self.vintage,
            supersedes: None,
            snapshot_sha: [0; 32],
            locator: serde_json::Value::Null,
            parser: String::new(),
        })
    }

    /// The exact decimal of `value_text`, or `None` when it is not a number.
    pub fn dec(&self) -> Option<news_ledger::decimal::Dec> {
        let (numeric, _) = news_ledger::decimal::parse(&self.value_text).ok()?;
        news_ledger::decimal::Dec::from_numeric(&numeric)
    }
}

impl From<&news_ledger::DataPoint> for Point {
    fn from(p: &news_ledger::DataPoint) -> Self {
        Self {
            dp_id: p.dp_id.clone(),
            series_key: p.series_key.clone(),
            label: p.label.clone(),
            value_text: p.value_text.clone(),
            unit: p.unit.clone(),
            currency: p.currency.clone(),
            scale: p.scale.clone(),
            seasonal_adj: p.seasonal_adj.clone(),
            period_label: p.period_label.clone(),
            period_start: p.period_start,
            period_end: p.period_end,
            period_type: p.period_type,
            fiscal_year: p.fiscal_year,
            fiscal_period: p.fiscal_period.clone(),
            vintage: p.vintage,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub doc_id: i64,
    pub blocks: Vec<Block>,
}

/// One `text_blocks` row. `text` is untrusted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub block_no: i32,
    /// `heading`, `paragraph`, `quote` … as the parser recorded it; `None` when it did not.
    pub kind: Option<String>,
    pub text: String,
}
