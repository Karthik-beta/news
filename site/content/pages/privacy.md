---
title: Privacy
description: What News collects when you read it, what the error-report form stores, and which third parties see your visit.
updated: 2026-11-16
---

## In short

News has no accounts and sets no tracking cookies. Its analytics are cookieless. The error-report form stores only what you type, and an e-mail address is optional. Images and video load from other sites, and those sites receive your IP address.

## Accounts and cookies

News has no accounts, no comments and no ads. It does not use tracking cookies.

TODO(Karthik): check whether the site stores anything in your browser, such as a light or dark theme choice, and say so here if it does.

## Analytics

News measures visits with Cloudflare Web Analytics. It is cookieless, and Google Analytics is not used.

TODO(Karthik): the design says to confirm that the current Cloudflare beacon sets no cookies, and to use server-side zone analytics instead if it does (Technical Design, open items). Confirm before launch, then say which one is used. Also say what the analytics record and how long Cloudflare keeps it.

## Images and video from other sites

Images and video on a page are not stored on News's servers. Each one loads from its source, and that source receives your IP address when it does.

YouTube videos use the privacy-enhanced address, `youtube-nocookie.com`, and load only after you choose to play them. Video that cannot be embedded is linked, and the site you reach by the link has its own privacy policy.

Source links go to other sites, which have their own privacy policies. News does not control them.

## The report an error form

The [report an error](/report-an-error) form stores what you type and a little more. It stores:

- the text of your report;
- the kind of error you chose: figure, wording, link, image or other;
- the page the report is about;
- the optional note on where on the page the error is;
- your e-mail address, if you gave one;
- the language of the page and the time the report was received.

It does not store your IP address, your browser's user-agent string or any other detail of your request. Your IP address is used only to limit how many reports one sender can make in a minute, and it is held in memory for that purpose.

Reports are stored in a database at Cloudflare, separate from the one that holds the pages. Each report expires 180 days after it is received. Expired reports are no longer read, and they are deleted the next time Karthik collects reports.

### Who reads a report

Karthik reads the reports. An AI tool (Claude) may read the report text and the "where" note, to sort and group the reports. It is given no tools, and it treats the text as data. No correction is published without Karthik's approval.

Your e-mail address is kept so Karthik can reply to you. When reports are processed, the address is not copied into Karthik's local working files.

Do not put personal information in the text of a report.

TODO(Karthik): say how long the local copy of a report's text and page is kept after the report has been dealt with. The design says only that the e-mail is not copied locally.

### Limits on reports

The form takes a limited number of reports a day for the whole site. When the limit is reached, the form says so, and you can send the report the next day (UTC).

## Where the data is held

News runs on Cloudflare (the CDN, file storage, the search and report databases, and the site's servers) and on Amazon Web Services (the database that holds the pages). Cloudflare, as the network that serves the site, handles each visit to it.

TODO(Karthik): confirm after the first deployment that no IP address or user agent appears in the logs of the site's servers, then add a sentence on server logs. The Workers' design says invocation logs are off, and asks for one real log line to be checked after deployment.

## Your choices

TODO(Karthik): say how a reader can ask for a report they sent to be deleted before the 180 days are up, and name the legal basis and the place where requests are handled, if you want to cite one. The documents give no process and no jurisdiction.

## Contact

For privacy questions, see [contact](/contact).
