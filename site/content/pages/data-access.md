---
title: Data access
description: How to read, download and query News pages, feeds and data, and the limits that apply.
updated: 2026-11-16
---

## In short

All content is free to read, cite, download and query, by people, AI tools and MCP clients, as long as you respect the usage limits on this page. Static pages are the cheapest to fetch. Heavy users should use the bulk dumps rather than crawl the site.

## Formats for every page

Every page is available in these formats at the same path.

| Format | Path | Use |
| --- | --- | --- |
| HTML | the page's address | People |
| Markdown | the page's address plus `index.md` | Language models and readers who want plain text |
| JSON | the page's address plus `data.json` | The page's data, its layout specification and its source records |
| CSV | linked from each chart | The data behind a chart |

Each page links to its Markdown and JSON versions.

## Sitewide files

- **`llms.txt`** describes the site, its sections and how to fetch content.
- **XML sitemaps** list only the pages offered to search engines.
- **RSS and Atom feeds** cover the whole site and each category.
- **`robots.txt`** states the expected use of the site.

TODO(Karthik): confirm what `robots.txt` and `llms.txt` say, and add the feed addresses once the routes are final.

## Search

The site has a search page that covers titles, summaries, entities and tags, with filters for category, region, type and date. Search runs on a free-tier database with a daily limit. When the limit is reached, search is unavailable until the day's count resets, and the page says so and points to browsing by category.

## MCP server

News runs a read-only MCP server so AI tools can query the site. It follows the MCP specification revision 2026-07-28, keeps no sessions, and runs no AI.

TODO(Karthik): add the server's address and a short example of adding it to an MCP client. The address is not set in the design documents.

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

- The MCP server answers 30 requests a minute for each client. The limit applies at each Cloudflare location, so it is approximate.
- Repeated identical MCP requests are answered from a cache for a short time.
- MCP searches are limited to 8 words, and a list of an entity's pages holds at most 20.
- Search, MCP and the dynamic pages (the home page, hubs and the "Latest" list) are cached at the edge, so a repeated request may show a slightly older result.

TODO(Karthik): these are the launch values in the Workers' design (ADR 0006). Render them from config so the page cannot go out of date, and confirm them after the first deployment.

Clients that exceed a limit are refused. Please do not crawl the site continuously.

## Bulk dumps

For anyone who wants everything, a periodic archive of all pages' JSON and Markdown is planned, so that one download replaces crawling the site.

TODO(Karthik): the host and schedule for bulk dumps are undecided (Technical Design lists Hugging Face Datasets with an Internet Archive mirror, or GitHub Releases). Add the address and schedule when settled, or say that dumps are not yet available.

## Licence and credit

Original text, charts and page designs are CC BY 4.0, which means you may reuse them with credit. Credit the page you reuse and link to it.

TODO(Karthik): give the attribution wording you prefer for reuse.

Data keeps the licence of its source. Each page's sources panel shows the licence and the credit each source requires. Where a source does not allow its data to be redistributed, the page links to the source and does not offer the data as a file.

Images are not stored on News's servers. They are loaded from their sources, which set their licences. The caption of each image gives its credit and licence.

See the [legal page](/legal) for more.

## Accuracy

Built from public sources with AI and checked automatically. It may contain errors; check the linked source before relying on a figure. To report a mistake, use [report an error](/report-an-error).
