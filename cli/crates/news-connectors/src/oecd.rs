//! OECD Data Explorer (SDMX REST, JSON). No key. The service allows 60 downloads an hour, so the
//! registry paces it at one request every two minutes (Contacts lint: at most half the documented
//! rate); each followed series costs two requests (probe, history).

use crate::sdmx::{self, Rules, split_spec};
use crate::stat::{StatSeries, StatSource};
use crate::{ItemRef, ParseError, Parsed, Request, Snapshot};

pub const ID: &str = "oecd";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://sdmx.oecd.org/public/rest/data/";
const HISTORY: u32 = 12;

pub const SERIES: &[StatSeries] = &[
    StatSeries {
        id: "CPI.USA",
        country: "us",
        family: "cpi_yoy",
        spec: "OECD.SDD.TPS,DSD_PRICES@DF_PRICES_ALL,1.0/USA.M.N.CPI.PA._T.N.GY",
        entity: ("iso3166", "US", "United States"),
    },
    StatSeries {
        id: "CLI.USA",
        country: "us",
        family: "cli",
        spec: "OECD.SDD.STES,DSD_STES@DF_CLI/USA.M.LI...AA...H",
        entity: ("iso3166", "US", "United States"),
    },
];

const RULES: Rules = Rules {
    unit: &["UNIT_MEASURE", "UNIT"],
    seasonal: &["ADJUSTMENT"],
    multiplier: &["UNIT_MULT"],
    decimals: &["DECIMALS"],
};

pub struct Oecd;

fn url(s: &StatSeries, last: u32) -> String {
    format!("{BASE}{}?lastNObservations={last}&format=jsondata", s.spec)
}

impl StatSource for Oecd {
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
    fn page(&self, _s: &StatSeries) -> String {
        "https://data-explorer.oecd.org/".into()
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
