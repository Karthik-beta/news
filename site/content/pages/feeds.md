---
title: RSS and Atom feeds
description: Follow News in a feed reader, for the whole site or for one category.
updated: 2026-11-23
---

## Follow the whole site

Add either address to a feed reader:

- RSS: [{site.origin}/rss.xml](/rss.xml)
- Atom: [{site.origin}/atom.xml](/atom.xml)

Both list the newest pages first.

## Follow one category

Every category has its own feeds. Add `/rss.xml` or `/atom.xml` to the end of the category's address. For a category at `{site.origin}/example`, the feeds are `{site.origin}/example/rss.xml` and `{site.origin}/example/atom.xml`. A sub-category works the same way.

## What an entry holds

Each entry is one page: its headline, its summary, a link to it, and the times it was first published and last updated. Every published page appears in a feed, including pages that are not offered to search engines.

The [data access page](/data) lists the other ways to read News with a program.

<!-- TODO(Karthik): approve this copy. Its sentences are a first draft and have not had the Opus rewrite that reader-facing pages get. -->
