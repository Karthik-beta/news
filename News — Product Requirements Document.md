# News — Product Requirements Document

Nov 14, 2026 · @Karthik

## Overview

**News** (working name) is a free, static website at `news.karthik.nz` that turns major company reports, economic releases, institutional reports, science findings and major news into visual, fully sourced pages built with OpenUI.

**Problem.** News sites and official reports are text-heavy. Quarterly results, IMF outlooks and central-bank decisions arrive as dense PDFs and formal prose, and most coverage restates them in more text with stock photos.

**Product.** Each page presents the same public information as charts, dashboards, stat cards, timelines, embedded official video and source-linked images. Every figure is cited to its public source.

**Positioning.** *Major company, economic and science reports, turned into visual, fully sourced pages, open to people and AI alike.*

**Nature of the project.** A personal hobby project run by one person. Pages are produced at scale (100+ per run, 2–3 runs a day) by AI agents on the author's laptop, with automated quality checks. There is no monetization, no paid data, no paid services and no accounts. The long-term future (brand, app) is deliberately left open. Technical details are in the separate technical design document.

## Goals, non-goals and success

### Goals

1. Make major reports and news faster and more interesting to read through visuals.
2. Cite every figure to a public, free source so any reader or AI can verify it.
3. Be truly open: readable, citable and downloadable by people, AI tools and MCP clients.
4. Run on free tiers and fit one person's part-time capacity.

### Non-goals

- No market prices, price charts or trading data.
- No predictions, forecasts of our own, ratings or investment opinions.
- No breaking-news coverage or attempt to cover every story.
- No paid data sources or paid APIs.
- No accounts, comments, paywall, ads or monetization.
- No localization in v1 (English only).

### Success measures

| Measure | How it is observed |
| --- | --- |
| More readers | Visits and returning visitors in Cloudflare Web Analytics |
| More citations | Backlinks, referrals from AI tools and search, MCP server calls, bulk-dump downloads |
| Personal reputation | Mentions and shares tied to the author's name; qualitative |

No numeric targets are set yet (see Risks and open questions).

## Target reader

The reader is someone like the author: global, English-reading, technically and financially literate, and curious across tech, business, finance, economy and science.

- **Regions they follow:** North America, Europe, India, China, other Asian countries and Australia.
- **What they want:** the substance of a report in minutes, with the numbers visible and the source one tap away.
- **What they don't need:** basic definitions on every page (a glossary covers those), opinions on what to buy, or hype.
- **Secondary readers:** AI assistants, agents and MCP clients that fetch and cite pages on behalf of people.

## Content scope

Coverage is selective but wide: the largest companies (S&P 500 scale and regional equivalents), flagship institutional reports, major policy decisions, major news and notable science. A designed content system scores, clusters and selects items breadth-first across categories, regions and page types; there is no fixed inclusion rule. Backfill covers the current year only.

### Categories

Tech, Business, Finance, Economy and Science at launch. Categories and sub-categories are stored as data, so new ones can be added, split or renamed at any time without changing page URLs. A page can belong to several categories.

### Page types

| Page type | Built from | Typical visuals |
| --- | --- | --- |
| Earnings (quarterly / annual results) | Company filings and XBRL data | Revenue-to-profit Sankey, segment split, margin trend, guidance vs actual |
| Macro release | Statistics agency and central-bank data APIs | Time series, regional comparison, change cards |
| Major report | Institutional reports (IMF, World Bank, OECD, BIS and similar) | Key-figure dashboard, forecast tables as published, chapter summaries |
| Policy decision | Central-bank and government statements, press-conference video | Decision card, rate path, quote highlights, embedded official video |
| News story | Primary sources, with news outlets linked for context | Timeline, key-fact cards, map, source panel |
| Science | Papers, preprints, agency releases and open datasets | Finding cards, data charts, imagery, method summary |
| Explainer | Any of the above | Diagrams and annotated charts |

### Tiers for company reports

- **Tier 1 (full):** a full visual dashboard, written commentary, and video when the source includes one. For the most important companies and reports.
- **Tier 2 (standard):** the main figures and charts with a short summary.
- **Queued:** not yet covered; filled in by later runs as capacity allows.

Other page types follow the same idea: flagship items get full treatment, the rest a standard template.

### Science

Science pages present the source information as published, with no extra editorial filtering. Each page records its source type (peer-reviewed paper, preprint, agency release, dataset) as part of the citation.

## Sourcing and licensing

Every fact comes from a public, free source; no paid service or paid API is ever used. Numbers are shown as the source states them: original currency, original units (metric or imperial) and the company's own fiscal period labels, with no conversion.

### Source rules

1. **Primary sources first.** Filings, official statistics, institutional reports, papers, press releases and official statements are the basis of every page.
2. **News outlets as links only.** For news stories, outlets may be cited as linked context. Their text and images are never copied.
3. **Licence recorded per source.** A source registry stores each source's licence (public domain, CC BY, custom terms) and required attribution, so every page shows the correct credit automatically.
4. **Unclear redistribution terms** (for example some exchange announcement sites) are linked and summarised, not re-published as data files.

### Example free sources by region

| Region | Companies | Economy and data |
| --- | --- | --- |
| United States | SEC EDGAR (XBRL API) | FRED, BLS, BEA, Census, Federal Reserve |
| Canada | SEDAR+ (no API) | Statistics Canada, Bank of Canada Valet API |
| Europe | filings.xbrl.org (ESEF), UK Companies House | Eurostat, ECB Data Portal, ONS, Bank of England |
| India | BSE filings (XBRL) | RBI DBIE, MOSPI, data.gov.in |
| China | HKEX filings | NBS (English), PBoC |
| Japan, Korea, Singapore | EDINET API, DART API | SingStat, MAS, BoJ |
| Australia | ASX announcements | ABS API, RBA |
| Global | n/a | IMF, World Bank, OECD, BIS, WTO, WHO |
| Science | n/a | arXiv, OpenAlex, Crossref, Europe PMC, PubMed, bioRxiv/medRxiv, NASA, ESA, NOAA, USGS, CERN Open Data |

### Images

Images are **never stored** on the site's infrastructure. Each image is saved as a URL pointing at its source, with its credit, licence and alt text, and loaded from that URL when the page is viewed.

- Images come from the source material itself or from free-to-use sources, chosen by code from an allow-list of sources with known licences: public-domain government imagery (NASA and similar), Wikimedia Commons (licence read from its API), Unsplash and official press kits. News outlets' images are never used.
- Each image record holds: source URL, source page, credit, licence, alt text and the date it was last checked.
- If an image fails to load, the page shows a neutral placeholder with the alt text and credit, so the layout never breaks.
- Charts and diagrams are drawn from data, so many pages need no external image at all.

### Video

Video is embedded only when the source material itself includes it: the page re-uses the same embed the source used (company, central-bank, agency and institution channels), with privacy-enhanced YouTube embeds (`youtube-nocookie.com`) that load on click. Video that cannot be embedded, such as many earnings-call webcasts, is linked.

## Page experience

Every page is built once by Claude agents on the author's laptop as OpenUI Lang, passes automated checks, and is served as a static page. Sonnet chooses the layout freely to suit the content; a large on-brand component library and a small fixed base keep the site consistent.

### Fixed base on every page

- Title, one-line summary, categories, and a "Data as of" date.
- Byline: "Built with Claude (AI) · Automated checks · Run by Karthik", linking to the methodology page.
- Citation behaviour, number and unit formatting, colour palette and chart style.
- Sources panel, "Report an error" link and, when a page has been updated, an "Updated on …" note.
- Links to the page's Markdown and JSON versions.

### Flexible per page

Which components appear and in what order: dashboards, KPI and stat cards, charts (line, bar, Sankey, waterfall, maps), timelines, comparison tables, quote highlights, image galleries and video embeds. The component library holds hundreds of components built on open-source OpenUI; OpenUI Dashboards (a paid managed product) is not used.

### Citations

1. **Every number carries its source.** Each figure in the spec references a source record: URL, document, page number or XBRL tag, and retrieval date. Hovering or tapping a figure shows it.
2. **Sources panel per page.** Lists every source with its type (filing, dataset, report, paper, preprint, press release, news outlet), licence and attribution.
3. **Primary vs secondary labelled.** Primary sources and linked news outlets are visually distinct.
4. **Downloadable data.** The data behind each chart is available as CSV and JSON.

### Presentation rules

- Currency, units and fiscal periods exactly as in the source; a fiscal period is labelled as the company labels it.
- Facts and any commentary are visually separated.
- A glossary explains metrics; pages link terms to it instead of repeating definitions.
- Mobile-first layout; charts remain readable on a phone.

## Information architecture

Content is organised by category and sub-category, with hub pages that collect related pages and a global search across everything.

### Navigation

- **Home:** a dynamic, curated front page. A curation agent picks the lead story, the side stories and each section's stories every 2–3 days (see [Home page curation](#home-page-curation)); an automatic "Latest" list updates with every publish in between. Plus entry points to each category. The home page is assembled on the server from the stored curation; curating it never rebuilds or redeploys the site.
- **Category and sub-category hubs:** for example Finance → Earnings, Economy → Central banks, Science → Space.
- **Entity hubs:** one per company, country, central bank or institution, listing all its pages over time. Each quarter's page stays frozen; the hub rolls forward.
- **Region filter** across hubs (North America, Europe, India, China, Asia, Australia, Global).
- **Tags** for cross-cutting themes (AI, energy, trade, climate).
- **Global search** across titles, summaries, entities and tags, served by the site's search API.

### URL scheme

URLs are permanent and never change once published. **Page URLs carry no category**, so categories can be added, split or renamed without breaking links; only hub URLs contain category slugs, and renamed hubs get redirects.

| Page | Example URL |
| --- | --- |
| Earnings | `/2026/11/apple-fy26-q4-results` |
| Macro release | `/2026/11/us-cpi-october-2026` |
| Major report | `/2026/10/imf-world-economic-outlook-october-2026` |
| Policy decision | `/2026/10/rbi-monetary-policy-october-2026` |
| News story or science | `/2026/11/<slug>` |
| Category hub | `/finance`, `/finance/earnings` |
| Entity hub | `/companies/apple`, `/countries/india` |
| Machine versions | same path + `index.md` and `data.json` |

**Languages:** English is served at the root. Every record carries a language field now, so future languages can be added under `/<lang>/` prefixes without changing English URLs.

**Rebrand:** if the site moves to a new domain, every `news.karthik.nz` URL redirects permanently (301) to the same path on the new domain, and `karthik.nz` stays registered.

## Open access

All content is free to read, cite, download and query by people, AI tools and MCP clients, as long as they respect the published usage and rate limits.

### Formats for every page

| Format | Purpose |
| --- | --- |
| HTML | People; rendered OpenUI page |
| Markdown (`index.md`) | LLMs and readers who want plain text |
| JSON (`data.json`) | The page's data, OpenUI spec and source records |
| CSV | Per-chart data downloads |

### Site-wide machine access

- `llms.txt` describing the site, its sections and how to fetch content.
- XML sitemap, plus RSS/Atom feeds for the whole site and each category.
- **MCP v2 server (v1).** A small serverless endpoint so AI agents can query the site. Proposed tools: search pages, get a page (Markdown or JSON), list categories and entities, get an entity's pages, get a chart's data. It follows the stateless MCP v2 spec (revision 2026-07-28), is read-only, and runs no AI.
- **Bulk data dumps (v1).** A periodic archive of all pages' JSON and Markdown, so anyone wanting everything downloads one file instead of crawling.

### Fair use and rate limits

- A data-access page and `robots.txt` state the expected usage and point heavy users to the bulk dumps.
- Article pages are static files on the CDN; dynamic pages (home, hubs, latest) are cached at the edge for hours or days, so heavy traffic cannot run up database usage.
- CDN rate limiting protects the MCP endpoint and the site from continuous hammering.

### Licence

- Original text, charts and page designs: **CC BY 4.0** (free to reuse with credit).
- Underlying data keeps its original source licence, shown in each page's sources panel.

## Trust and editorial

Trust rests on three things: the author's real identity on a personal domain, a citation on every figure, and an open statement of how pages are made.

- **Authorship.** Every page states plainly how it was made: built with Claude (AI), checked automatically, run by Karthik, with a link to the author's main site.
- **Methodology page.** Explains the process (public sources → code fetches and parses data → Claude writes the page → automated checks and sample checks → publish), the source rules, the no-conversion policy for units and periods, and the licence.
- **Numbers come from data, not the AI.** Pages reference figures by ID and code fills in the values from the source data. Some figures, such as a change from a year earlier or a share of a total, are calculated by News from the source's own figures, only between like-for-like values (same unit, currency, scale and comparable periods), and are labelled "Calculated by News from …" with the formula recorded (Bureau design, decision D5). The methodology page says so.
- **Report an error.** A small link on every page opens a short form. When something is fixed, the page shows "Updated on …".
- **Accuracy note.** A brief, low-key note in the footer and methodology page: content is built from public sources with AI and may contain errors; check the linked source.
- **Disclaimers.** "Not investment advice" on finance and economy pages; "Not medical advice" on health-related science pages.
- **Neutral tone.** Facts and any commentary are separated; no predictions or recommendations.

## Content production

Pages are produced at scale by Claude Code agents on the author's laptop, by the local system called Bureau ([Bureau design](News%20—%20Bureau%20Design.md)). Runs that call a model start only when the author types `/bureau` in Claude Code; the main run is `/bureau run daily`. A local scheduler may run code-only background jobs (release-calendar sync, capture of releases at their scheduled time, source discovery), never anything that calls a model or publishes (Bureau design, decision D8). The author's typed command is the approval step; there is no per-page manual review.

### How a run works

1. The author types `/bureau run daily` in Claude Code. `NEWS_TARGET_PAGES` (default 100) suggests how many pages to make; it is not a hard limit.
2. A designed content system discovers, scores, clusters and selects items breadth-first across categories, regions and page types. Scheduled releases are captured on time by code from the official release calendars, so they are ready when the run starts.
3. Code fetches and parses all source data (APIs, XBRL, PDFs) before any AI sees it, and builds a compact brief per item.
4. Claude writes each page: Sonnet 5.5 for layout and text, Haiku 5.5 for lighter tasks. Each page is a small, separate call to keep token use low.
5. Code renders the page, runs automated checks, and Haiku spot-checks a sample.
6. Code publishes the page, updates hubs and search, and applies the daily indexing budget.
7. Local working files are pruned; the knowledge graph, ledger and telemetry are kept, and the raw source files behind every figure are kept for a retention period so each figure stays provable (Bureau design, decision D7).

### Cadence and volume

- 100+ pages per run, 2–3 runs a day; roughly 10k+ pages in the first month and 250k+ in a year.
- Backfill covers the current year only.
- Pages are updated in place when sources revise data (restatements, revised statistics).

### Home page curation

The home page is curated, not just a feed of the newest pages.

- **When:** every 2–3 days the author runs the Frontpage workflow in Claude Code with one command, `/bureau curate home`. There is no fixed schedule; if a run is missed, the last curation stays live.
- **What the agent decides:** the lead story, the two side stories, which sections appear and in what order, the 3–4 stories in each section, and which existing key figure or mini chart each story shows. It picks for importance, freshness and breadth across categories, regions and page types, with Tier 1 pages first.
- **What it never does:** write new headlines, summaries or commentary. Every story on the home page uses its own page's fly title, headline, summary and figures, so the home page carries the same citations and wording as the page itself.
- **Approval:** the workflow checks the curation automatically, shows the author a local preview, and publishes only after the author approves it by typing `/bureau approve` in the session. Every published curation is versioned and can be rolled back.
- **Server side, no rebuild:** the curation is stored as data in the cloud database. The home page is rendered on request from that data and cached at the edge; publishing a curation only stores the new version and clears the home page cache. Nothing is rebuilt or redeployed.
- **Between curations:** curated slots stay as chosen. The "Latest" list updates on every publish. If a curated page is withdrawn, its slot is filled automatically from the best recent pages. The region filter shows the curated stories for that region, topped up with its latest pages.

### Consistency

Page-type skills, a brand skill and a component library built with Opus 5.5 keep pages consistent while layouts adapt to content. Full details are in the technical design document.

## Technical architecture

The full design is in News — Technical Design. In short:

- **All work and all AI run on the author's laptop:** fetching, parsing, Claude agents, rendering, telemetry. The cloud only stores and serves pages and runs no AI.
- **Local stack:** a Rust `news` CLI for CPU-heavy work, Bun and Astro for rendering, PostgreSQL with pgvector for the queue and knowledge graph, and the existing ClickHouse (separate database) for telemetry.
- **Cloud:** Cloudflare R2 serves article pages as static files; AWS DynamoDB is the database of record; Cloudflare D1 holds the search index; Cloudflare Workers run the dynamic pages (home, hubs, latest, categories), the write API, search and the MCP v2 server.
- **No site rebuilds:** each page is rendered on the laptop and uploaded on its own; dynamic pages are cached at the edge for hours or days and refreshed only where a new page affects them. The home page's curation is data, published by the Frontpage workflow and read by the home page at request time.
- **Free tiers only**, with paid upgrades considered only when the site grows.

## Distribution

All free channels are used. v1 relies on search, feeds and manual sharing; automation comes later.

| Channel | v1 | Later |
| --- | --- | --- |
| Search engines (sitemap, structured data, Google Search Console, IndexNow) | Yes |  |
| RSS/Atom feeds (site and per category) | Yes |  |
| AI access (`llms.txt`, Markdown/JSON, MCP server) | Yes |  |
| Bulk data dumps | Yes |  |
| Internet Archive submission per page | Yes |  |
| Social posts with chart images (X, LinkedIn, Bluesky, Mastodon, Reddit, Hacker News) | Manual | Auto-posting |
| Email newsletter |  | Phase 3 (free tier only) |
| Author's main site `karthik.nz` | Link and feature |  |

### Search visibility and domain reputation

Publishing and indexing are separate decisions, to protect the reputation of `karthik.nz`. Google publishes no safe page count; its scaled-content policy judges whether each page serves a distinct purpose.

- Every page is published and open to people, AI tools and MCP clients.
- Only a daily budget of pages (`NEWS_INDEX_PER_DAY`, starting low, around 20) is offered to search engines; the rest are `noindex` and out of the sitemap until promoted.
- Tier 1 pages and pages with the most unique data and visuals are indexed first.
- The budget rises in steps only while Search Console shows pages being indexed.

## Legal, privacy and accessibility

### Legal

- Disclaimers: not investment advice; not medical advice where relevant.
- Attribution for every source, following its licence (from the source registry).
- No copying of news outlets' text or images; links only.
- A contact address for corrections and takedown requests.
- Careful, factual wording about named companies and people.

### Privacy

- No accounts, no tracking cookies; cookieless analytics only.
- YouTube embeds load only on click (click-to-load), using `youtube-nocookie.com`.
- The privacy note states that images and videos load from third-party sources, which receive the visitor's IP address.
- The error-report form stores only what the reader types; an email address is optional. Analytics use Cloudflare Web Analytics, checked to set no cookies; Google Analytics is not used.

### Accessibility

- Target WCAG 2.2 AA.
- Every chart has a text summary and an accessible data table or CSV link.
- Colour-blind-safe palettes; meaning never carried by colour alone.
- Alt text on every image; keyboard navigation; mobile-first layout.

## Roadmap

Phases run in order with no fixed dates; each starts when the previous one's exit criteria are met.

1. **Phase 0 — Foundations.** Local services (PostgreSQL, OTel Collector, ClickHouse database), Cloudflare and AWS accounts with budget alarms, and a spike proving OpenUI pages render to static HTML.
   - Exit: one page rendered end to end and events reaching ClickHouse.
2. **Phase 1 — Design and components.** Agree the design direction with the author (using the `frontend-design` skill), then build the component library with Opus 5.5.
3. **Phase 2 — Content system and tooling.** Design the content system with Opus 5.5; build the Rust `news` CLI, first source connectors, orchestration (including the home page curation workflow) and telemetry.
4. **Phase 3 — Launch (v1).** Cloud services live (static pages, dynamic hubs, curated dynamic home page, search, MCP v2, Report an error, analytics); first runs at a low indexing budget.
   - Exit: site live on `news.karthik.nz`, runs producing 100+ pages each, and the home page curated at least once through the workflow.
5. **Phase 4 — Scale.** Raise volume and indexing budget using telemetry and Search Console; bulk dumps; email newsletter (free tier); social sharing.
6. **Phase 5 — Future (undecided).** Mobile app rendering the same OpenUI pages, proper branding and domain (with 301 redirects), localization.

## Risks and open questions

### Risks

| Risk | Effect | Mitigation |
| --- | --- | --- |
| Search engines treat the volume as scaled content | Domain reputation harmed | Separate publishing from indexing; low indexing budget raised gradually; unique data and visuals on every page |
| Wrong figure published (units, period, sign, wrong data field) | Errors carry the author's name | Numbers filled from source data by code; automated checks on every page; Haiku sample checks; Report an error and "Updated on" |
| Free-tier limits reached | Search fails for the day, throttled writes, or billing | Static pages bypass compute; edge caching; sharded search; budget alarms; usage tracked in the local dashboard |
| OpenUI cannot render on the server | Pages harder for search engines and AI to read | Early spike in Phase 0; fallback plan before building the library |
| Free public APIs change or go offline | Fetch failures | One connector per source; failures logged and retried |
| Hotlinked images break or are blocked | Missing visuals | Allow-listed sources; placeholder with alt text and credit; periodic link check |
| Unclear licence for some sources | Redistribution risk | Licence in the source registry; link and summarise instead of republishing data |
| Claude usage limits on large runs | Smaller runs | Lean prompts, cached instructions, small briefs; runs resume where they stopped |
| Hobby priority drops | Site goes stale | Static pages stay online; pages archived in the Internet Archive |
| Home curation is skipped for a while | Home page looks dated | "Latest" list keeps updating on every publish; withdrawn pages are replaced automatically; the local dashboard flags a curation older than 4 days |

### Open questions

- Numeric targets for readers and citations, if any.
- Bulk-dump host (Hugging Face Datasets with an Internet Archive mirror, or GitHub Releases) and schedule.
- How to handle restated figures on pages already indexed.
- Final brand name, if the site leaves "News".
- Newsletter service that fits the free-tier rule.
- The design direction (to be agreed with the author before any design work).
