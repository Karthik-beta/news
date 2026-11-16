---
title: Data access
description: How to read, download and query News pages, feeds and data, how News fetches its own sources, and the limits that apply.
updated: 2026-11-16
---

## In short

All content is free to read, cite, download and query, by people, AI tools and Model Context Protocol (MCP) clients, as long as you respect the usage limits on this page. Static pages are the cheapest to fetch. If you want a lot of pages, please do not crawl the site continuously. Bulk dumps, a single download of every page, are planned but not yet available.

## Formats for every page

Every page is available in these formats at the same path.

| Format | Path | Use |
| --- | --- | --- |
| HTML | the page's address | People |
| Markdown | the page's address plus `index.md` | Language models and readers who want plain text |
| JSON | the page's address plus `data.json` | The page's data, its layout specification and its source records |
| CSV | linked from each chart | The data behind a chart |

For example, for the page at `/2026/11/us-cpi-october-2026`, the Markdown version is at `/2026/11/us-cpi-october-2026/index.md` and the JSON version is at `/2026/11/us-cpi-october-2026/data.json`.

Each page links to its Markdown and JSON versions.

## Sitewide files

- **`llms.txt`** describes the site, its sections and how to fetch content.
- **XML sitemaps** list only the pages offered to search engines.
- **RSS and Atom feeds** cover the whole site and each category.
- **`robots.txt`** states the expected use of the site.

<!-- TODO(Karthik): confirm what `robots.txt` and `llms.txt` say, and add the feed addresses once the routes are final. -->

## Search

The site has a search page that covers titles, summaries, entities and tags, with filters for category, region, type and date. Search runs on a free-tier database with a daily limit. When the limit is reached, search is unavailable until the day's count resets, and the page says so and points to browsing by category.

## MCP server

News runs a read-only MCP server so AI tools can query the site. It follows the MCP specification revision 2026-07-28, keeps no sessions, and runs no AI. Clients that use the older revisions 2025-11-25 and 2025-06-18 also work. Requests from a web page on another site are refused.

<!-- TODO(Karthik): add the server's address and a short example of adding it to an MCP client. The address is not set in the design documents. -->

The tools are all read-only.

| Tool | Returns |
| --- | --- |
| `search_pages` | Matching pages with title, summary and address |
| `get_page` | A page as Markdown or JSON |
| `list_categories` | The category tree |
| `get_entity` | An entity, such as a company or a country, and its pages |
| `get_data` | The data behind a page or a chart |

Figures are returned exactly as published. When a reply is too large, it is cut, the cut is flagged, and a link to the full file is included.

## Limits

News runs on free tiers, so limits apply. They start low and may be raised or changed later.

- The MCP server answers {mcp.ratePerMinute} requests a minute for each IP address. The count is kept at each Cloudflare location separately, so the limit is approximate: a client that reaches the site from several locations can send more.
- Repeated identical MCP requests are answered from a cache for a short time.
- MCP searches are limited to {mcp.searchMaxWords} words, and a list of an entity's pages holds at most {mcp.entityPageLimit}.
- Search, MCP and the dynamic pages (the home page, hubs and the "Latest" list) are cached at the edge, so a repeated request may show a slightly older result.

<!-- TODO(Karthik): render the limits above from config so the page cannot go out of date. Launch values in ADR 0006 are 30 requests a minute, 8 words and 20 pages. Confirm them after the first deployment. -->

Clients that exceed a limit are refused.

## How News fetches sources

News fetches its own sources, such as filings and statistics, with a program that identifies itself in the User-Agent header as `News/{version} (+https://news.karthik.nz/data-access)`. It follows each site's `robots.txt` and rate limits, makes conditional requests so that it does not download a file that has not changed, and obeys `Retry-After` when a site asks it to slow down.

If you run a site that News fetches and you would like fewer requests, [contact](/contact) Karthik.

<!-- TODO(Karthik): render the User-Agent from config and check it matches what the fetcher sends. The documents describe the fetcher as polite (conditional requests, rate limits, Retry-After, robots.txt) but do not give the User-Agent string. -->

## Bulk dumps

Bulk dumps are planned: a periodic archive of all pages' JSON and Markdown, so that one download replaces crawling the site. They are not available yet.

<!-- TODO(Karthik): the host and schedule for bulk dumps are undecided (Technical Design lists Hugging Face Datasets with an Internet Archive mirror, or GitHub Releases). Add the address and schedule when settled. -->

## Licence and credit

Original text, charts and page designs are CC BY 4.0, which means you may reuse them with credit. Credit the page you reuse and link to it.

<!-- TODO(Karthik): give the attribution wording you prefer for reuse. -->

Data keeps the licence of its source. Each page's sources panel shows the licence and the credit each source requires. Where a source does not allow its data to be redistributed, the page links to the source and does not offer the data as a file.

Images are not stored on News's servers. They are loaded from their sources, which set their licences. The caption of each image gives its credit and licence.

See the [legal page](/legal) for more.

## Accuracy

Built from public sources with AI and checked automatically. It may contain errors; check the linked source before relying on a figure. To report a mistake, use [report an error](/report-an-error).
