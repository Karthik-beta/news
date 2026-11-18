//! Australian Bureau of Statistics Data API (SDMX REST, JSON 2.0). No key. Dimensions are the
//! dataflow's own (`MEASURE`, `TSEST`, `REGION`): the unit is the `MEASURE` name and the seasonal
//! adjustment the `TSEST` name, as the ABS writes them.

use crate::sdmx::{self, Rules, split_spec};
use crate::stat::{StatSeries, StatSource};
use crate::{ItemRef, ParseError, Parsed, Request, Snapshot};

pub const ID: &str = "abs";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://data.api.abs.gov.au/rest/data/";
const HISTORY: u32 = 8;

pub const SERIES: &[StatSeries] = &[StatSeries {
    id: "CPI.AUS",
    country: "au",
    family: "cpi",
    spec: "CPI/1.10001.10.50.Q",
    entity: ("iso3166", "AU", "Australia"),
}];

const RULES: Rules = Rules {
    unit: &["MEASURE", "UNIT_MEASURE"],
    seasonal: &["TSEST"],
    multiplier: &["UNIT_MULT"],
    decimals: &["DECIMALS"],
};

pub struct Abs;

fn url(s: &StatSeries, last: u32) -> String {
    format!("{BASE}{}?lastNObservations={last}&format=jsondata", s.spec)
}

impl StatSource for Abs {
    fn id(&self) -> &'static str {
        ID
    }
    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }
    fn catalogue(&self) -> &'static [StatSeries] {
        SERIES
    }
    fn probe(&self, s: &StatSeries) -> Request {
        Request::sdmx(url(s, 1))
    }
    fn history(&self, s: &StatSeries) -> Vec<Request> {
        vec![Request::sdmx(url(s, HISTORY))]
    }
    fn page(&self, s: &StatSeries) -> String {
        format!(
            "https://www.abs.gov.au/statistics/economy/price-indexes-and-inflation/consumer-price-index-australia/latest-release#{}",
            split_spec(s.spec).0
        )
    }
    fn parse_series(
        &self,
        s: &StatSeries,
        item: &ItemRef,
        snaps: &[Snapshot],
        parser: &str,
    ) -> Result<Parsed, ParseError> {
        sdmx::parse(
            s,
            item,
            crate::util::find(snaps, &url(s, HISTORY))?,
            parser,
            &RULES,
            &format!("{ID}:{}", split_spec(s.spec).0),
        )
    }
}
