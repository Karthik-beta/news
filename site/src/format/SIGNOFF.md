# Formatters — test sign-off

Status: **signed off**

Tester: Opus 5.5 component tester (not the builder) · Builder: Opus 5.5 component builder (issue #4) · Date: 16 November 2026
Branch / commit: `phase-1/site/renderer`, formatters at b842525

Scope: the shared formatters in `src/format/` ("Formatting" area of `news-component-testing`), tested on their own
(builder's unit and property tests, reviewed) and as they render inside the primitives (tester's tests through
`SourceRef`, its citation popover and `RefText`, in jsdom, in plain Bun and in Chromium, Firefox and WebKit).

## Formatting, against design/content-style.md

| Rule | Result | Tests |
|---|---|---|
| Currencies: $, €, £, ₹, ₩, NT$, HK$, A$, C$, S$; anything else as ISO code and a space (CHF 4.5bn) | ☑ pass | `src/components/SourceRef/SourceRef.format.test.tsx` (13 currencies, compact and as reported); `format.test.ts` |
| Yen and yuan: ¥ only when the subtitle names the currency, otherwise the word | ☑ pass | `SourceRef.format.test.tsx`; `format.test.ts` |
| Never converted: same currency, unit and scale word in the popover ("$391,035 million", "420 million Persons") | ☑ pass | `format.property.test.ts` (round trip); `SourceRef.format.test.tsx` |
| m / bn / trn with no space, one decimal place, never more precision than the source ("$2bn" stays "$2bn"), 999.96bn becomes 1.0trn | ☑ pass | both |
| Rounding half away from zero, exactly (no floats) | ☑ pass | `format.test.ts`; `SourceRef.format.test.tsx` |
| True minus (U+2212) everywhere, before the currency (−$5.7bn, −CHF 4.5bn); a value that rounds to zero is unsigned | ☑ pass | both; plus every primitive fixture checked for a hyphen-minus before a digit in `tests/ssr/primitives.test.tsx` |
| Thousands separators and a decimal point (167,045; 46.2) | ☑ pass | both |
| %, percentage points ("2.1 points", "1 point"), basis points ("−25 basis points") | ☑ pass | both |
| Zero (0%, $0), very large (≥ 1e12: $123,456.8trn, 8.1trn) and very small (0.000000012) values | ☑ pass | both; edge-case fixture page in three engines |
| Missing values: "No data" (or "no data" inside a sentence), never a zero | ☑ pass | `SourceRef.format.test.tsx`; `SourceRef.spec.ts` |
| Dates: 1 November 2024, no comma; date-times as their UTC calendar date ("2026-11-13T23:30:00-05:00" → 14 November 2026) | ☑ pass | both |
| Fiscal and calendar period labels verbatim (fiscal 2024, Q4 FY26, fourth quarter of fiscal 2024, Q3 2024) | ☑ pass | both |
| Ranges with an en dash, "About" for approximations, change words ("Up 2.0% on fiscal 2023", "Unchanged") | ☑ pass | `format.test.ts` |
| Rendered on screen: the formatted figure in the right face, size and colour on every width, both themes, three engines | ☑ pass | `tests/e2e/components/SourceRef.spec.ts` |

## Issues found

None in the formatters. (The print and landscape defects found while testing cited figures are in SourceRef's and
Popover's sign-offs; they are not formatting problems.)

## Notes for Karthik (not blockers)

- In compact style, a value in a unit the house style does not know is written without its unit ("0.000000012" for a
  value in mol/L; "167,045" for Persons). The popover's as-reported line names it ("0.000000012 mol/L"). This follows
  content-style.md (the unit appears once, in the subtitle or the sentence), but a page writer has to name the unit in
  the sentence; worth confirming.
- As-reported "other" units are appended verbatim, so a unit such as "Index 1982-84=100" reads "314.5 Index
  1982-84=100" in the popover. Correct by the "never converted, verbatim" rule; noted for wording.

## Test files

- `src/format/format.test.ts`, `src/format/format.property.test.ts` (builder's; reviewed, kept)
- `src/components/SourceRef/SourceRef.format.test.tsx` (tester: the formatters inside the primitives)
- `tests/ssr/primitives.test.tsx` (no hyphen-minus before a digit in any fixture's server HTML)
- `tests/e2e/components/SourceRef.spec.ts` (rendered values on every screen)

## Sign-off

☑ Every formatting rule above passes.
Signed: Opus 5.5 component tester, 16 November 2026
