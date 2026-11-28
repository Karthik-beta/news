# ADR 0013: Proof and the templated fallback

- Status: proposed
- Date: 2026-11-25
- Issue: #17 (Phase 5, wave 5): render, save, Proof and the seven fixture pages
- Context documents: Bureau Design §3.9 (the Copy, A1/A2), §3.10 (Proof, C1–C14), decision D10; Technical Design "Quality checks"; `cli/crates/news-proof/README.md` (the check table)

## Context

A page writer's Copy holds words and references only; code fills in every figure. Between the Copy and a published page sit three pieces of code with no model in them: the validators every AI step's `news ai accept` runs, the render through Plate, and the checks C1–C14 on the rendered bundle. Bureau §3.10 lists the checks; it leaves open where each runs, what a failure does, how a page that keeps failing still gets published (D10), and how the whole chain is proved on real data before Phase 6. #79, #83 and the fixture-pages PR built it; this record writes down the decisions they made.

## Decisions

### Two gates, one word list

- **Before render (validators, `news_proof::validate`).** `news save` and `news ai accept` check the Copy alone: `page-draft.v1` schema, every reference in the brief, the C4 numeric-literal lint, banned words, lengths, and (through Plate's `/check`) the OpenUI parse and props against the component specs. A refused Copy is written to `rejected.json` with its errors, which is the input to the A2 repair call. `news save` skips the parse and props so it runs without Bun; it still refuses an image credit or licence typed in `page.oui` (the renderer takes them from the image record).
- **After render (Proof, `news check`).** C1–C14 run on the bundle in `work/pages/<story>/`, the files Press would send. A blocking failure is `validation_failed` (exit 6) with the Markup in `details`; C12 only warns. One row per check per page goes to ClickHouse `news.check_results`.
- The word lists live once, in `data/seeds/proof-v1.toml` (C4 quantities and units, C5 direction words, disclaimers, image hosts, size and component ranges) and `data/seeds/banned-words.toml` (the list the `news-brand` skill gives the writer). The validators' C4 lint and Proof's C4 should read the same list; `news_proof::lint` still holds its own copy (open, below).

### Everything outside the bundle comes in through `checks::Env`

Link checks (C10, cached 24 h in `work/cache/proof-links.json`), axe (C13, every Tier 1 page and one Tier 2 page in ten by a stable hash of the page ID), the page index (C14) and the stored text blocks (C6) are passed in. Every check therefore has a passing and a failing fixture with no network. `NEWS_PROOF_LINKS=off` and `NEWS_PROOF_AXE=off` skip C10 and C13 on a machine without network or browser, and the Markup says "skipped", never "pass".

### Render is idempotent

`news render` reads the stored Copy, the brief, `data.json`, `sources.json` and `page.json`, renders through Plate and assembles `meta.json` and `manifest.json` in Rust. It reads no clock: a new page takes the Copy's `saved_at`, and a re-save of the same Copy keeps it. The same Copy, data and renderer version give the same bundle SHA, which a test asserts. The renderer version is recorded in `meta.json` and the document's `<meta name="generator">`.

### Code adds what the writer must not

The byline, "Data as of", the sources list and the disclaimer are code's (the `news-brand` skill tells the writer not to write them). The brief's `rules.disclaimer` (`not-investment-advice`, `not-medical-advice`, set by Fixer from the categories and page type) goes to Plate as `disclaimer`, and the renderer puts the Disclaimer component after the sources. C9 then checks the visible text against the categories in `meta.json`, so a page whose brief and categories disagree fails.

### The templated fallback (D10)

- Only for **Tier 2**: when the Copy fails the validators after the A2 repair, `news ai fallback` builds a Copy from the brief by code (`news_proof::fallback`): a page-type headline pattern with references, KeyFigures, the main chart and a data table, and no prose.
- It is stored flagged `templated` and rendered `noindex` whatever `page.json` says. The index budget never promotes it; a later run that gets a good Copy replaces it.
- A Tier 1 Copy that fails twice is re-queued, never templated: Tier 1 pages carry the site's reputation.
- The templated page goes through the same render and the same C1–C14 as any page.

### The seven fixture pages prove the chain

- One page per page type, in `pipeline/fixtures/` laid out as a work directory (`briefs/`, `pages/<type>/`), so `NEWS_WORK_DIR=pipeline/fixtures news render|check <type>` works on them.
- Code builds each brief, `data.json`, `sources.json` and `page.json` from recorded source files parsed by the connectors and briefed by Fixer (`news_fixer::page::data` writes `data.json`). Only recorded data whose licence allows it is used: BLS and SEC (public domain), the World Bank (CC BY 4.0), NASA, and the ECB and Federal Reserve, whose data the pages cite but do not offer for download (C8).
- The Copies (`pipeline/fixtures/copies/`) are written by Opus with references only. A test asserts that no Copy holds a digit outside a reference, except an image record's URL, width and height, which must equal the brief's.
- The rendered bundles are committed. The Rust tests (no Bun in that CI job) check the committed bundles with C1–C14 and rebuild the inputs; with Bun they re-render each page to the committed bundle byte for byte, and build a templated page for a failing Tier 2 fixture.
- Karthik reviews the seven pages side by side before Phase 3 exits.

## Consequences

- A change to a component, a formatter or the renderer that alters a fixture page shows up as a bundle diff in review; `NEWS_UPDATE_GOLDEN=1` rewrites them deliberately.
- `data.json` is Fixer's to write; Press and Rewrite write `page.json` for updates (version, first publication).
- Open: `news_proof::lint` (the validators) and `proof-v1.toml` (Proof) each hold a C4 word list; the lint should read the seed file.
- Chart JSON downloads join the bundle: `manifest.v1` accepts `charts/<id>.json` beside `charts/<id>.csv`, so the JSON link every chart shows resolves once published (ADR 0010 already reads `charts/<id>.json` from R2). C8 checks both for values of a source that does not allow redistribution.
