---
title: Report an error
description: Tell News about a wrong figure, wording, link or image on a page.
updated: 2026-11-16
---

## Tell News what is wrong

Pages are built from public sources with AI and checked automatically, and they may contain errors. If you find one, write to Karthik at the address on the [contact page](/contact). Check the linked source first if you can, and say what the source shows.

## What to include

- **The page.** The address of the page with the error. Copy it from your browser's address bar.
- **What is wrong.** Describe it in your own words. Say what the page shows and what the source shows.
- **Where on the page.** For example, a chart title or a table row.

## Form text

These are the labels and messages for the form.

| Item | Text |
| --- | --- |
| Button | Send report |
| After sending | Report sent |
| After sending, detail | Your report has been received. If you gave an e-mail address, Karthik may reply to it. |
| Text too short | The report must be at least {report.text.minLength} characters. Say what is wrong and what the source shows. |
| Text too long | The report must be at most {report.text.maxLength} characters. Shorten it and send it again. |
| Where note too long | The note on where the error is must be at most {report.where.maxLength} characters. |
| E-mail not valid | This e-mail address does not look valid. Check it, or leave the field empty. |
| Page not found | No published page has this address. Check the address and send the report again. |
| Too many from one sender | Too many reports were sent in a short time. Wait a minute and send the report again. |
| Over the daily limit | Reports are closed until 00:00 UTC because today's limit was reached. Send your report after that time. |
| Not sent | The report could not be sent. Try again in a moment. |

<!-- TODO(Karthik): the detail line and the messages above are drafts. Approve them. The form shows these words (report.astro reads this table), so edit them here and nowhere else. -->

<!-- Builder notes (not reader text):
- Limits are rendered from contracts/cloud/report.schema.json, not typed here: {report.text.minLength}, {report.text.maxLength}, {report.text.maxBytes}, {report.where.maxLength}. The text counter must enforce both characters and UTF-8 bytes ({report.text.maxBytes}).
- {reports.retentionDays} comes from the report Worker's retention period. TODO(Karthik): export it from the Worker.
- When the page is opened without a page, the form needs a field for the page address, because the report intake requires a page ID.
- The daily limit and the per-minute limit both return 429. Tell them apart with the Retry-After header: about a minute means the per-minute limit, and a longer value means the daily limit, which resets at 00:00 UTC.
-->

<!-- TODO(Karthik): render the report limits from contracts/cloud/report.schema.json, so this page cannot drift from the form. -->
## What happens next

Karthik collects the reports and checks them. If an error is confirmed, the page is fixed and shows "Updated on …". Not every report leads to a change, and a reply is not guaranteed. A fix prompted by a report is not published without Karthik's approval. The [corrections policy](/corrections) has the steps.

## What the form stores

The form stores the text of your report, the page, the optional "where" note, your e-mail address if you gave one, the page's language and the time it was received. It does not store your IP address or your browser's user-agent string. Reports expire {reports.retentionDays} days after they are received. The [privacy page](/privacy) has the details.

## Other ways to reach Karthik

For anything other than an error on a page, see [contact](/contact).
