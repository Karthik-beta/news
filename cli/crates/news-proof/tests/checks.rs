//! Contract tests for C1-C14: a passing bundle (`tests/fixtures/good`) and, for each check, a failing one made
//! by changing that bundle. No network: link checks and axe are fakes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use news_proof::axe::{AxeRunner, Violation};
use news_proof::bundle::Bundle;
use news_proof::checks::{AxeMode, Duplicate, Env, Markup, run};
use news_proof::lexicon::Lexicon;
use news_proof::links::{LinkChecker, Probe};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn good() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/good")
}

/// A copy of the good bundle that the test then breaks.
struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("news-proof-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("charts")).unwrap();
        for e in walk(&good()) {
            let rel = e.strip_prefix(good()).unwrap().to_owned();
            std::fs::copy(&e, dir.join(rel)).unwrap();
        }
        Fixture(dir)
    }
    fn edit(&self, file: &str, from: &str, to: &str) -> &Self {
        let p = self.0.join(file);
        let t = std::fs::read_to_string(&p).unwrap();
        assert!(t.contains(from), "{file} has no {from:?}");
        std::fs::write(p, t.replacen(from, to, 1)).unwrap();
        self
    }
    /// Change a JSON file through `serde_json::Value`.
    fn patch(&self, file: &str, f: impl FnOnce(&mut serde_json::Value)) -> &Self {
        let p = self.0.join(file);
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        f(&mut v);
        std::fs::write(p, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        self
    }
    fn write(&self, file: &str, text: &str) -> &Self {
        std::fs::write(self.0.join(file), text).unwrap();
        self
    }
    fn remove(&self, file: &str) -> &Self {
        std::fs::remove_file(self.0.join(file)).unwrap();
        self
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

struct Links {
    dead: Vec<&'static str>,
    asked: RefCell<Vec<String>>,
}
impl LinkChecker for Links {
    fn check(&self, url: &str) -> Probe {
        self.asked.borrow_mut().push(url.to_owned());
        let ok = !self.dead.contains(&url);
        Probe {
            ok,
            note: if ok { "HTTP 200" } else { "HTTP 404" }.into(),
        }
    }
}

struct Axe(Result<Vec<Violation>, String>);
impl AxeRunner for Axe {
    fn run(&self, _: &Path) -> Result<Vec<Violation>, String> {
        self.0.clone()
    }
}

fn violation(impact: &str) -> Violation {
    Violation {
        id: "color-contrast".into(),
        impact: Some(impact.into()),
        help: "Elements must have sufficient colour contrast".into(),
        nodes: 3,
    }
}

struct World {
    blocks: BTreeMap<(i64, i64), String>,
    dups: Option<Vec<Duplicate>>,
    links: Links,
    axe: Axe,
    axe_mode: AxeMode,
}

impl World {
    fn new() -> Self {
        World {
            blocks: BTreeMap::from([(
                (5521, 3),
                "Shelter: The index for shelter was the largest contributor to the increase."
                    .into(),
            )]),
            dups: Some(Vec::new()),
            links: Links {
                dead: Vec::new(),
                asked: RefCell::new(Vec::new()),
            },
            axe: Axe(Ok(Vec::new())),
            axe_mode: AxeMode::Auto,
        }
    }
    fn run(&self, dir: &Path) -> Markup {
        let lex = Lexicon::v1().unwrap();
        let env = Env {
            lexicon: &lex,
            text_blocks: &self.blocks,
            duplicates: self.dups.as_deref(),
            links: Some(&self.links),
            axe: &self.axe,
            axe_mode: self.axe_mode,
        };
        run(&Bundle::load(dir), &env)
    }
}

fn status(m: &Markup, check: &str) -> String {
    m.checks
        .iter()
        .find(|c| c.check == check)
        .unwrap()
        .status
        .clone()
}

/// The failure messages of one check.
fn failures(m: &Markup, check: &str) -> Vec<String> {
    m.failures
        .iter()
        .filter(|f| f.check == check)
        .map(|f| format!("{}: {}", f.at, f.message))
        .collect()
}

/// Run the broken bundle and assert that only `check` failed, with a message containing `needle`.
fn breaks(f: &Fixture, check: &str, needle: &str) {
    let m = World::new().run(&f.0);
    let found = failures(&m, check);
    assert!(
        found.iter().any(|s| s.contains(needle)),
        "{check} should fail with {needle:?}; failures: {:#?}",
        m.failures
    );
    assert_eq!(status(&m, check), "fail");
    assert!(!m.pass);
}

#[test]
fn the_good_bundle_passes_every_check() {
    let w = World::new();
    let m = w.run(&good());
    assert!(m.failures.is_empty(), "{:#?}", m.failures);
    assert!(m.warnings.is_empty(), "{:#?}", m.warnings);
    assert!(m.pass);
    let ids: Vec<&str> = m.checks.iter().map(|c| c.check.as_str()).collect();
    assert_eq!(
        ids,
        [
            "C1", "C2", "C3", "C4", "C5", "C6", "C7", "C8", "C9", "C10", "C11", "C12", "C13", "C14"
        ]
    );
    for c in &m.checks {
        assert_eq!(c.status, "pass", "{}: {:?}", c.check, c.note);
    }
    // Link checks ran on the source; the relative image and the same-site paths were not probed.
    assert_eq!(
        *w.links.asked.borrow(),
        ["https://www.bls.gov/news.release/cpi.nr0.htm"]
    );
}

#[test]
fn the_markup_serialises_to_the_contract_shape() {
    let m = World::new().run(&good());
    let v = serde_json::to_value(&m).unwrap();
    assert_eq!(v["pass"], true);
    assert_eq!(v["page_id"], "us-cpi-october-2026");
    assert_eq!(v["failures"], serde_json::json!([]));
    let f = Fixture::new();
    f.edit("page.oui", "Annual inflation\"", "Inflation in 2024\"");
    let v = serde_json::to_value(World::new().run(&f.0)).unwrap();
    assert_eq!(v["pass"], false);
    let first = &v["failures"][0];
    assert_eq!(first["check"], "C4");
    assert!(first["where"].as_str().unwrap().starts_with("page.oui:"));
    assert!(first["message"].is_string());
}

// C1: parse and render.

#[test]
fn c1_a_missing_file_or_broken_markup_fails() {
    let f = Fixture::new();
    f.remove("index.md");
    breaks(&f, "C1", "index.md: file is missing");
    let f = Fixture::new();
    f.edit(
        "page.oui",
        "Page([figures, prose, chart])",
        "Page([figures, prose, chart)",
    );
    breaks(&f, "C1", "unmatched");
    let f = Fixture::new();
    f.edit("page.oui", "\"dp_cpi_2026_10\"", "dp_cpi_2026_10");
    breaks(&f, "C1", "not quoted");
    let f = Fixture::new();
    f.write("data.json", "{ not json");
    breaks(&f, "C1", "not valid JSON");
    let f = Fixture::new();
    f.edit("index.html", "<article", "<div");
    breaks(&f, "C1", "no <article>");
}

// C2: references resolve.

#[test]
fn c2_an_unknown_reference_or_a_missing_source_fails() {
    let f = Fixture::new();
    f.edit("page.oui", "{dp_cpi_2026_10} in", "{dp_cpi_2099_01} in");
    breaks(&f, "C2", "`dp_cpi_2099_01` is not in data.json");
    let f = Fixture::new();
    f.patch("data.json", |v| {
        v["points"]["dp_cpi_2026_10"]["source"] = "src_missing".into()
    });
    breaks(&f, "C2", "not in sources.json");
    let f = Fixture::new();
    f.edit(
        "page.oui",
        "said [shelter]",
        "said dp_cpi_2026_09 [shelter]",
    );
    breaks(&f, "C2", "written without braces");
    let f = Fixture::new();
    f.edit("index.html", "<button class=\"source-ref\"", "<span class=\"source-ref source-ref--missing\" data-ref=\"dp_x\">No data</span><button class=\"source-ref\"");
    breaks(&f, "C2", "shows \"no data\"");
}

// C3: values match source.

#[test]
fn c3_a_wrong_derived_value_or_a_value_that_is_not_shown_fails() {
    let f = Fixture::new();
    f.edit("data.json", "\"value\": \"-0.3\"", "\"value\": \"-0.4\"");
    breaks(&f, "C3", "recomputing pp_change");
    let f = Fixture::new();
    f.edit("data.json", "\"value\": \"2.4\"", "\"value\": \"2.5\"");
    breaks(&f, "C3", "is not the value of the source's text");
    let f = Fixture::new();
    f.patch("data.json", |v| {
        v["points"]["dp_cpi_2026_09"]["scale"] = "millions".into()
    });
    breaks(&f, "C3", "inputs differ in scale");
    // The page shows 2.9 where the data point says 2.4.
    let f = Fixture::new();
    f.edit(
        "index.html",
        "<span class=\"popover__value\">2.4%</span>",
        "<span class=\"popover__value\">2.9%</span>",
    )
    .edit(
        "index.html",
        "data-ref=\"dp_cpi_2026_10\">2.4%",
        "data-ref=\"dp_cpi_2026_10\">2.9%",
    );
    breaks(&f, "C3", "does not show that figure");
}

// C4: numeric-literal lint.

#[test]
fn c4_blocks_digits_and_written_out_quantities_without_a_reference() {
    for (bad, needle) in [
        ("revenue was 2024 USD", "a digit in prose"),
        ("it was three per cent", "written-out quantity \"three\""),
        ("sales doubled", "multiplier \"doubled\""),
    ] {
        let f = Fixture::new();
        f.edit(
            "page.oui",
            "said [shelter]",
            &format!("said {bad} [shelter]"),
        );
        breaks(&f, "C4", needle);
    }
    // The head text is Copy too.
    let f = Fixture::new();
    f.edit(
        "meta.json",
        "The figures come from",
        "The 2024 figures come from",
    );
    breaks(&f, "C4", "meta.json:summary");
}

#[test]
fn c4_allows_names_with_digits_and_references() {
    let f = Fixture::new();
    f.edit("page.oui", "said [shelter]", "said the G7 and COVID-19 and one of the Formula 1 teams [shelter]")
        .edit("meta.json", "\"entities\": [", "\"entities\": [{ \"id\": \"ent_f1\", \"name\": \"Formula 1\", \"kind\": \"organisation\" },");
    let m = World::new().run(&f.0);
    assert_eq!(status(&m, "C4"), "pass", "{:#?}", m.failures);
}

#[test]
fn c4_exempts_a_templated_page() {
    let f = Fixture::new();
    f.edit(
        "meta.json",
        "The figures come from",
        "The 2024 figures come from",
    );
    breaks(&f, "C4", "meta.json:summary");
    f.patch("meta.json", |v| {
        v["flags"] = serde_json::json!(["templated"])
    });
    let m = World::new().run(&f.0);
    assert_eq!(status(&m, "C4"), "pass", "{:#?}", m.failures);
}

// C5: direction words.

#[test]
fn c5_a_direction_word_must_agree_with_the_sign() {
    let f = Fixture::new();
    f.edit("page.oui", "down {dv_cpi_change}", "up {dv_cpi_change}");
    breaks(&f, "C5", "\"up\" does not agree with `dv_cpi_change`");
    let f = Fixture::new();
    f.edit("meta.json", "The rate fell by", "The rate held at");
    breaks(&f, "C5", "\"held\"");
    // A positive change: "fell" and "down" disagree, "rose" and "up" agree.
    let f = Fixture::new();
    f.patch("data.json", |v| {
        v["derived"]["dv_cpi_change"]["value"] = "0.3".into()
    });
    breaks(&f, "C5", "\"fell\" does not agree");
    f.edit("page.oui", "down {dv_cpi_change}", "up {dv_cpi_change}")
        .edit("page.oui", "America fell to", "America rose to")
        .edit("meta.json", "The rate fell by", "The rate rose by");
    let m = World::new().run(&f.0);
    assert!(failures(&m, "C5").is_empty(), "{:#?}", m.failures);
}

// C6: quotes verbatim.

#[test]
fn c6_a_quote_must_be_an_exact_substring_of_its_text_block() {
    let f = Fixture::new();
    f.edit(
        "data.json",
        "was the largest contributor\",",
        "was the biggest contributor\",",
    );
    breaks(&f, "C6", "not an exact substring");
    let f = Fixture::new();
    f.patch("data.json", |v| {
        v["quotes"]["q_shelter"]["locator"]["block"] = 9.into()
    });
    breaks(&f, "C6", "is not stored");
    let f = Fixture::new();
    f.patch("data.json", |v| {
        v["quotes"]["q_shelter"]["locator"] = serde_json::json!({"kind": "text_block"})
    });
    breaks(&f, "C6", "names no stored text block");
}

// C7: period and unit labels.

#[test]
fn c7_labels_are_the_sources_and_a_series_is_one_measure() {
    let f = Fixture::new();
    f.edit(
        "data.json",
        "\"pd_2026_10\": { \"label\": \"October 2026\" }",
        "\"pd_2026_10\": { \"label\": \"Oct 2026\" }",
    );
    breaks(&f, "C7", "should show as \"Oct 2026\"");
    let f = Fixture::new();
    f.patch("data.json", |v| {
        v["series"]["sr_cpi"]["unit"] = "Index".into()
    });
    breaks(
        &f,
        "C7",
        "has unit \"Percent\" but the series says \"Index\"",
    );
}

// C8: licence.

#[test]
fn c8_attribution_is_required_and_restricted_data_is_not_offered() {
    let f = Fixture::new();
    f.edit("index.html", "id=\"source-src_bls_cpi\"", "id=\"x\"");
    breaks(&f, "C8", "does not list this source");
    let f = Fixture::new();
    f.edit(
        "sources.json",
        "\"name\": \"Public domain\"",
        "\"name\": \"\"",
    );
    breaks(&f, "C8", "`licence.name` is empty");
    let f = Fixture::new();
    f.edit(
        "sources.json",
        "\"redistribution\": true",
        "\"redistribution\": false",
    );
    breaks(&f, "C8", "does not allow redistribution");
    // The chart JSON offers the same values.
    f.remove("charts/sr-cpi.csv").write(
        "charts/sr-cpi.json",
        "{\"rows\": [{\"period\": \"October 2026\", \"value\": \"2.4\"}]}\n",
    );
    breaks(&f, "C8", "does not allow redistribution");
    // Without the downloads, a restricted source is fine: cited, not redistributed.
    f.remove("charts/sr-cpi.json");
    let m = World::new().run(&f.0);
    assert_eq!(status(&m, "C8"), "pass", "{:#?}", m.failures);
}

// C9: required fields.

#[test]
fn c9_title_summary_categories_alt_text_and_disclaimers() {
    let f = Fixture::new();
    f.edit(
        "meta.json",
        "\"summary\": \"The rate",
        "\"summary\": \"\", \"x\": \"The rate",
    );
    breaks(&f, "C9", "meta.json:summary");
    let f = Fixture::new();
    f.edit("meta.json", "[\"economy/inflation\"]", "[]");
    breaks(&f, "C9", "needs at least one category");
    let f = Fixture::new();
    f.edit(
        "index.html",
        "alt=\"Line chart of annual inflation\"",
        "alt=\"\"",
    );
    breaks(&f, "C9", "has no alt text");
    let f = Fixture::new();
    f.edit("index.html", "Not investment advice.", "");
    breaks(&f, "C9", "needs the disclaimer \"Not investment advice.\"");
    // A page outside finance and economy needs none.
    f.edit("meta.json", "[\"economy/inflation\"]", "[\"technology\"]");
    let m = World::new().run(&f.0);
    assert_eq!(status(&m, "C9"), "pass", "{:#?}", m.failures);
}

// C10: links and media.

#[test]
fn c10_a_dead_source_blocks_and_a_dead_image_warns() {
    let f = Fixture::new();
    let mut w = World::new();
    w.links.dead = vec!["https://www.bls.gov/news.release/cpi.nr0.htm"];
    let m = w.run(&f.0);
    assert_eq!(status(&m, "C10"), "fail");
    assert!(failures(&m, "C10")[0].contains("does not answer (HTTP 404)"));

    f.edit(
        "index.html",
        "src=\"/images/cpi.png\"",
        "src=\"https://images-assets.nasa.gov/a.jpg\"",
    );
    let mut w = World::new();
    w.links.dead = vec!["https://images-assets.nasa.gov/a.jpg"];
    let m = w.run(&f.0);
    assert_eq!(status(&m, "C10"), "warn");
    assert!(m.pass, "a missing image is a warning, not a block");
    assert_eq!(m.warnings[0].check, "C10");
}

#[test]
fn c10_images_only_from_the_allow_list_and_embeds_must_answer() {
    let f = Fixture::new();
    f.edit(
        "index.html",
        "src=\"/images/cpi.png\"",
        "src=\"https://tracker.example/pixel.png\"",
    );
    breaks(&f, "C10", "not from an allow-listed host");
    let f = Fixture::new();
    f.edit(
        "index.html",
        "<figure>",
        "<figure><iframe src=\"https://video.example/embed/1\"></iframe>",
    );
    let mut w = World::new();
    w.links.dead = vec!["https://video.example/embed/1"];
    let m = w.run(&f.0);
    assert!(failures(&m, "C10")[0].contains("the embed"));
}

// C11: banned words and tone.

#[test]
fn c11_the_shared_banned_list_is_blocked() {
    for (bad, word) in [
        ("inflation soared", "soared"),
        ("the company beat its own guidance", "beat"),
        ("it missed", "missed"),
        ("prices will rise", "will"),
        ("investors are poised to buy", "poised to"),
        ("we think so", "we think"),
    ] {
        let f = Fixture::new();
        f.edit(
            "page.oui",
            "said [shelter]",
            &format!("said {bad} [shelter]"),
        );
        breaks(&f, "C11", &format!("banned word or phrase \"{word}\""));
    }
    let f = Fixture::new();
    f.edit("page.oui", "said [shelter]", "said why? [shelter]");
    breaks(&f, "C11", "no exclamation marks or questions");
    // A banned word inside a quote is the source's, not ours: quotes come in by reference.
    let f = Fixture::new();
    f.patch("data.json", |v| {
        v["quotes"]["q_shelter"]["text"] = "The index for shelter".into()
    });
    let m = World::new().run(&f.0);
    assert_eq!(status(&m, "C11"), "pass", "{:#?}", m.failures);
}

#[test]
fn c4_skips_media_fields_but_not_their_prose() {
    let f = Fixture::new();
    f.edit("page.oui", "chart = ", "video = VideoEmbed(\"Press conference on the decision\", \"Federal Reserve\", \"https://www.federalreserve.gov/a20240918.htm\", \"pqHJyLAo3ac\", \"2024-09-18\")
chart = ");
    let m = World::new().run(&f.0);
    assert_eq!(status(&m, "C4"), "pass", "{:#?}", m.failures);
    let f = Fixture::new();
    f.edit("page.oui", "chart = ", "video = VideoEmbed(\"Press conference of 2024\", \"Federal Reserve\", \"https://www.federalreserve.gov/a.htm\", \"pqHJyLAo3ac\", \"2024-09-18\")
chart = ");
    breaks(&f, "C4", "a digit in prose");
}

// C12: size and structure (warnings).

#[test]
fn c12_warns_on_component_count_and_size() {
    let f = Fixture::new();
    f.write("page.oui", "root = Page([a])\na = Text(\"x\")\n");
    let m = World::new().run(&f.0);
    assert_eq!(status(&m, "C12"), "warn");
    assert!(m.pass, "C12 is a warning");
    assert!(m.warnings[0].message.contains("1 components"));
    let f = Fixture::new();
    f.write("charts/big.csv", &"x,y\n".repeat(300_000));
    let m = World::new().run(&f.0);
    assert!(
        m.warnings
            .iter()
            .any(|w| w.check == "C12" && w.message.contains("over the budget"))
    );
}

// C13: accessibility.

#[test]
fn c13_serious_and_critical_violations_block_and_tier_2_is_sampled() {
    let f = Fixture::new();
    let mut w = World::new();
    w.axe = Axe(Ok(vec![violation("serious"), violation("minor")]));
    let m = w.run(&f.0);
    assert_eq!(status(&m, "C13"), "fail");
    assert_eq!(failures(&m, "C13").len(), 1, "minor issues do not block");
    // axe that cannot run does not pass the page.
    w.axe = Axe(Err("no browser".into()));
    let m = w.run(&f.0);
    assert!(failures(&m, "C13")[0].contains("axe could not run: no browser"));
    // Tier 2: skipped unless the page is the sampled one in ten; it is stable per page id.
    let mut ran = 0;
    for i in 0..60 {
        f.patch("meta.json", |v| {
            v["tier"] = 2.into();
            v["id"] = format!("page-{i}").into();
        });
        let m = w.run(&f.0);
        if status(&m, "C13") != "skipped" {
            ran += 1;
        }
    }
    assert!((1..=15).contains(&ran), "{ran} of 60 Tier 2 pages ran axe");
    // Off.
    w.axe_mode = AxeMode::Off;
    let m = w.run(&good());
    assert_eq!(status(&m, "C13"), "skipped");
}

// C14: duplicate page.

#[test]
fn c14_a_page_with_the_same_slug_or_event_key_must_be_an_update() {
    let mut w = World::new();
    w.dups = Some(vec![Duplicate {
        page_id: "us-cpi-oct-2026".into(),
        slug: "2026/11/us-cpi-october-2026".into(),
        reason: "slug".into(),
    }]);
    let m = w.run(&good());
    assert_eq!(status(&m, "C14"), "fail");
    assert!(failures(&m, "C14")[0].contains("already exists with the same slug"));
    // The page index could not be read: not passed, not failed.
    w.dups = None;
    let m = w.run(&good());
    assert_eq!(status(&m, "C14"), "skipped");
    assert!(m.pass);
}
