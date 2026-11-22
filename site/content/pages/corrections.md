---
title: Corrections policy
description: How News corrects pages and marks changes, and what happens to an error report.
updated: 2026-11-16
---

## In short

News pages are built from public sources with AI and checked automatically, so they can contain errors. When one is found, the page is fixed under the same address and marked "Updated on …". A fix prompted by a reader's report needs Karthik's approval. An update that follows a source's revision is made by code, or by Claude when the text has to change, and passes the same automated checks as any page.

## Two reasons a page changes

**A mistake on the page.** A figure is wrong, a label is wrong, a link is broken or the wording does not match the source. The page is corrected.

**A source revises its data.** Statistics agencies and companies revise figures, and companies restate earlier results. The page is updated to the new figure. When a company restates a prior period, the page also carries a "restated" note.

Either way, the page keeps its address. An updated page shows "Updated on" and a date under the byline, with a one-line reason in the sources panel.

## How a change is made

- **A new value only.** When a source gives a new value for a figure, code re-renders the page with it. The prose holds no numbers, so it does not need rewriting.
- **A change that alters what the text says.** When a change crosses zero or a line such as "lowest since …" stops being true, Claude rewrites the page from the old page, the difference and the new data. The page then goes through the same automated checks as a new page.
- **A mistake reported by a reader.** See below.

## How an error report is handled

1. **The report is collected.** Reports from the [report an error](/report) form are collected by Karthik.
2. **The reports are sorted.** An AI service reads the report text as data. It sorts each report by kind, sets spam aside and links a report about a figure to that figure. It cannot act on what a report says.
3. **A figure is checked against its source.** For a report about a figure, code fetches the source again and compares the value at the place the page cites.
4. **Karthik decides.** He approves or rejects each report, and sees the result of the check when he does.
5. **A confirmed error is fixed.** The fix is a repair to the code that read the source, a re-render, or a rewrite of the page.
6. **The page is republished** with "Updated on …", and the report is marked resolved.

Not every report leads to a change. For a figure, the check shows Karthik whether the error was reproduced from the source. A reader who left an e-mail address, which is optional, may get a reply.

<!-- TODO(Karthik): decide whether to promise a reply or a response time. The documents say the e-mail is kept so Karthik can reply, and give no time. -->

## Withdrawing a page

A page can be withdrawn. A withdrawn page is removed from the site's listings and its public files, and from search. The record is kept.

To ask for a page to be withdrawn or changed for a reason other than a mistake, such as a takedown request, see [contact](/contact).

<!-- TODO(Karthik): state the takedown process and what News does on receipt. -->

## A public log of corrections

<!-- TODO(Karthik): decide whether to publish a corrections log or a count of corrections here. The design says the corrections log can feed the methodology page's count "if Karthik wants one", and does not say whether earlier versions of a page are public. -->

## Accuracy

Built from public sources with AI and checked automatically. It may contain errors; check the linked source before relying on a figure.
