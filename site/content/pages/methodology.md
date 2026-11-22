---
title: Methodology
description: How News pages are made, where the figures come from, and which rules every page follows.
updated: 2026-11-16
---

## In short

Every figure on a News page comes from a public source and is filled in by code. An AI model, Claude, drafts the prose and never types a number. Automated checks run on every page before it is published. Karthik runs the system and answers for the result.

The byline on every page has three parts, and each one is explained below.

- **Built with Claude (AI).** Claude drafts the text and chooses the layout.
- **Automated checks.** Code checks every page before it is published.
- **Run by Karthik.** Karthik starts the runs and sets the rules.

## How a page is made

1. **Public sources.** Each page starts from public, free sources: filings, official statistics, institutional reports, papers, press releases and official statements.
2. **Code fetches and parses the data.** Programs download the source files and read the figures out of them. No AI sees a source before this step is finished.
3. **Claude writes the page.** Claude is given a short brief built by code and drafts the text and layout. It refers to figures by ID, not by value.
4. **Code fills in the figures.** The page renderer replaces each reference with the value from the source data.
5. **Automated checks and sample checks.** Code checks every page. A second AI pass reviews a sample of pages against their briefs.
6. **Publish.** A page that passes every check is published. A page that fails is not.

There is no manual review of each page. Karthik approves a run by starting it, or by setting its schedule, and the checks decide which pages are published.

## Figures come from source data, never from the AI

Claude writes references such as "the revenue figure" and does not write the number. The renderer then fills in the value from the source data by code.

Code enforces this in several ways.

- Each figure on a page must match the value saved from the source. A page that does not match is not published.
- The prose Claude writes may not contain digits or written-out amounts (for example "three per cent" or "doubled") unless they are references that code fills in. Names that contain digits, such as G20 or COVID-19, are allowed.
- A word such as "rose" or "fell" must agree with the sign of the figure it describes.
- A quotation must be an exact passage of its source text.
- Source links must work. Image links and video embeds are checked as well.

Each figure records where it came from: the source URL, the document, the page number or the XBRL tag, and the date the data was retrieved. Hover over or tap a figure to see it. The sources panel on each page lists every source with its type, licence and required credit.

The raw source files behind the figures are kept for {raw.retainDaysData} days for data files and {raw.retainDaysDocs} days for documents. For every figure, News keeps for good the source address, a fingerprint of the file it was read from, and the exact place in that file where the figure sits.

<!-- TODO(Karthik): say how long raw source files are kept. The design settles on a set period but marks the exact number of days as pending (Technical Design, `NEWS_RAW_RETAIN_DAYS_DATA` and `NEWS_RAW_RETAIN_DAYS_DOCS`). The numbers are `raw.retainDaysData` and `raw.retainDaysDocs` in site/src/config/site.ts. -->
<!-- TODO(Karthik): confirm that the fingerprints are kept for good (Bureau design, decision D7, "hashes for ever") and that this should be stated publicly. -->



## Figures are never converted

News shows each figure as its source states it.

- **Currencies** stay in the currency of the source. A result reported in yen stays in yen.
- **Units** stay as published. Imperial units stay imperial and metric units stay metric.
- **Periods** keep the source's own labels. A fiscal year is labelled the way the company labels it, such as "fiscal 2024".
- **Scale and precision** stay as published. A table marked "as reported" keeps the source's scale. News does not add decimal places the source does not have.
- **Rounded figures** in headlines and prose are rounded from the source value by code, not retyped.

When the source gives a unit that code does not recognise, the unit is kept exactly as the source writes it. It is never guessed.

## Figures calculated by News

Some figures are not in the source. A change from a year earlier, a difference between two values and a share of a total are worked out by code from the source's own figures. These are labelled "[Calculated by News from …](glossary:calculated-by-news-from)", and the formula is recorded.

Code works out a change only when the inputs can be compared like for like. They must have the same unit, currency and scale, the same seasonal adjustment, and comparable periods, such as the same fiscal period a year apart. When any of these does not hold, the change is not calculated and the page does not mention it.

## Sources and licences

1. **Primary sources first.** Filings, official statistics, institutional reports, papers, press releases and official statements are the basis of every page.
2. **News outlets are linked, not copied.** On news-story pages, outlets are linked as context only. Their text and images are not copied, and a quotation from one is limited to a short sentence with attribution.
3. **A licence is recorded for every source.** The source registry stores each source's licence and the credit it requires, so every page shows the right attribution.
4. **Unclear terms mean a link and a summary.** Where a source's terms on redistribution are unclear, News links to it and summarises it. It does not republish the data as files.
5. **Primary and secondary sources are labelled differently** in the sources panel.

Every source is public and free. News pays for no data, and it runs on free hosting tiers.

### Images and video

Images are not stored on News's servers. Each image is saved as a link to its source, with its credit, licence and alt text, and is loaded from that source when you view the page. Images come from the source material itself or from an allow-list of sources with known licences. News outlets' images are not used. If an image does not load, the page shows a placeholder with the alt text and credit.

Video is embedded only when the source material itself includes it, and it is loaded only when you choose to play it. Video that cannot be embedded is linked.

## Page types and tiers

Pages are of these types: earnings, macro release, major report, policy decision, news story, science and explainer. Science pages present the source information as published, and record the source type (peer-reviewed paper, preprint, agency release or dataset) in the citation.

Company reports come in two tiers. **[Tier 1](glossary:tier-1-and-tier-2)** pages, for the most important companies and reports, have a full visual dashboard and written commentary, and video when the source includes it. **Tier 2** pages show the main figures and charts with a short summary. Other page types follow the same idea: flagship items get the full treatment and the rest a standard template.

## Commentary and what News does not do

Facts and commentary are kept apart. Anything explanatory in Claude's own words is in a note block with a heading and an attribution line, and it is never mixed into the facts. Notes explain; they do not predict, recommend or judge.

News does not publish market prices or trading data, forecasts of its own, ratings, or investment opinions. It reports a published forecast only when it names whose forecast it is. Finance and economy pages carry the line "Not investment advice." Health-related science pages carry "Not medical advice."

Coverage is selective. News does not try to cover every story or to report breaking news.

## The home page

The home page is curated, not just a list of the newest pages. A new curation is prepared every few days, using Claude to pick the stories. Claude does not write any new text for it. Each story on the home page shows its own page's headline, summary and figures, so the wording and the citations are the same as on the page itself. Karthik approves each curation before it goes live. Between curations, the "Latest" list updates as pages are published.

## Search engines and the sitemap

Every page is published and open to people and AI tools. Only some pages are offered to search engines. The others carry a `noindex` tag, which asks search engines not to list the page, and they stay out of the sitemap until they are promoted. Anyone with the address can still open them.

## Corrections

When a figure is wrong, or a source revises its data, the page is corrected or updated and shows "Updated on …" with a one-line reason in the sources panel. The [corrections page](/corrections) explains how, and the [report an error page](/report) says how to report one.

## Accuracy

Built from public sources with AI and checked automatically. It may contain errors; check the linked source before relying on a figure.

## Licence

Text and charts are {licence.name}. Data keeps its original source licence, shown on each page. The [legal page](/legal) has the details.
