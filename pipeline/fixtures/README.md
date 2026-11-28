# Fixture pages

One page per page type (Development Plan 3.5; issue #17), built and checked like a production page. This folder is a work directory, so the CLI runs on it as is:

```sh
NEWS_WORK_DIR=pipeline/fixtures news render earnings
NEWS_WORK_DIR=pipeline/fixtures news check earnings    # live link checks and axe
```

| Page | Recorded data | Licence |
| --- | --- | --- |
| `earnings` | Apple's fiscal 2024 Form 10-K facts, SEC companyfacts (`site/tests/support/sec/`) | Public domain |
| `macro-release` | BLS payrolls (`CES0000000001`) and unemployment rate (`LNS14000000`) | Public domain |
| `major-report` | World Bank, GDP (current US$), United States | CC BY 4.0 |
| `policy-decision` | ECB main refinancing rate (`FM.MRR`) | ECB terms: cited, no download (C8) |
| `news-story` | The Bank of Canada's interest rate announcement of 28 October 2026 (`centralbank/boc-statement-20261028.html`, through the `cb-boc` Stringer) | Bank of Canada terms of use: free to use, copy and distribute with attribution |
| `science` | NASA Image and Video Library, Artemis II launch | NASA media guidelines |
| `explainer` | BLS consumer price index (`CUUR0000SA0`) | Public domain |

- `briefs/`, and `data.json`, `sources.json`, `page.json` and `copy.json` in `pages/<page>/`, are written by code (`cli/crates/news-fixer/tests/fixture_pages.rs`) from the recorded files the connectors parse.
- `copies/<page>.json` are the page writer's Copies (Opus), references only: no number is typed.
- The rest of `pages/<page>/` is the bundle Plate rendered. The tests check it with C1–C14 and, with Bun, re-render it byte for byte.
- After a deliberate change (a component, the renderer, Fixer), rewrite everything with `NEWS_UPDATE_GOLDEN=1 cargo test -p news-fixer --test fixture_pages` (Bun installed) and review the diff.
- With `NEWS_FIXTURES=1`, the site serves each page at `/fixtures/pages/<page>` with its styles, for review side by side.
