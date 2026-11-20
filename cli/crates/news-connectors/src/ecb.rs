//! ECB Data Portal (SDMX 2.1 REST, JSON). No key, no contact e-mail.
//!
//! Discovery is the shared hash probe (`lastNObservations=1`); the history request returns the last
//! [`HISTORY`] observations. Units are the ECB's own `UNIT` names ("Percent per annum", "US dollar").

use crate::sdmx::{self, Rules, split_spec};
use crate::stat::{StatSeries, StatSource};
use crate::{ItemRef, ParseError, Parsed, Request, Snapshot};

pub const ID: &str = "ecb";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://data-api.ecb.europa.eu/service/data/";
const HISTORY: u32 = 12;
const EA: (&str, &str, &str) = ("eurostat-geo", "EA", "Euro area");

pub const SERIES: &[StatSeries] = &[
    StatSeries {
        id: "FM.MRR",
        country: "ea",
        family: "policy_rate",
        spec: "FM/B.U2.EUR.4F.KR.MRR_FR.LEV",
        entity: EA,
    },
    StatSeries {
        id: "EXR.USD",
        country: "ea",
        family: "eur_usd",
        spec: "EXR/D.USD.EUR.SP00.A",
        entity: EA,
    },
];

const RULES: Rules = Rules {
    unit: &["UNIT"],
    seasonal: &["ADJUSTMENT"],
    multiplier: &["UNIT_MULT"],
    decimals: &["DECIMALS"],
};

pub struct Ecb;

fn url(s: &StatSeries, last: u32) -> String {
    format!("{BASE}{}?lastNObservations={last}&format=jsondata", s.spec)
}

impl StatSource for Ecb {
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
        let (flow, key) = split_spec(s.spec);
        format!("https://data.ecb.europa.eu/data/datasets/{flow}/{flow}.{key}")
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
