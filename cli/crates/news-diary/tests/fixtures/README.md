# Diary fixtures

Tests never touch the network. All three files were **recorded on 18 Nov 2026** with the declared User-Agent and trimmed:

| File | Origin | Trimmed to |
| --- | --- | --- |
| `bls-schedule.ics` | `https://www.bls.gov/schedule/news_release/bls.ics` (www.bls.gov robots.txt allows `/schedule/`) | the 23 events dated Nov 2026 to Jan 2027, of 313 |
| `bea-release-calendar.ics` | `https://www.bea.gov/news/schedule/ics/online-calendar-subscription.ics` | the 23 events dated Oct 2026 to Feb 2027, of 169 |
| `fomccalendars-2026-2027.htm` | `https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm` | the 2026 and 2027 panels (16 meetings) |

The FOMC 2027 panel is the Fed's own "revised tentative schedule" of 12 Nov 2026. Awkward real cases: BLS stamps local time (`TZID=US-Eastern`) while BEA stamps UTC and folds long summaries over two lines; BEA's 4th-quarter advance estimate reads "4th Quarter and Year 2026"; the December FOMC meeting has an asterisk (projections).
