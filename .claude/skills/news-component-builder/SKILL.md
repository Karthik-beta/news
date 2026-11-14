---
name: news-component-builder
description: "How to build a News UI or chart component on OpenUI (@openuidev/react-ui) in site/src/components or site/src/charts. Use this whenever you create, extend or fix a component, a chart type, a page primitive, or a component spec/index entry for the `news component` CLI. Component building is Opus 5.5 work in this project."
---

# Building a News component

Components are the whole visual surface of 250,000 AI-laid-out pages. A component must look right on its own, next to any other component, at any width, in both themes, with any data the pipeline can throw at it. Build for that, not for the happy-path fixture.

**Model rule:** components are built by Opus 5.5 agents (Technical Design). Testing is done afterwards by a *different* Opus 5.5 agent using the `news-component-testing` skill.

Before starting, load the `news-design-system` skill and read the component's spec in `design/components.md` or `design/charts.md`.

## Folder layout

```
site/src/components/<ComponentName>/      (charts: site/src/charts/<ChartName>/)
├─ <ComponentName>.tsx        component (server-renderable React)
├─ <ComponentName>.client.tsx only if it has an interactive part (hydrated island)
├─ <ComponentName>.css        styles, tokens only
├─ index.ts                   defineComponent registration for the OpenUI library
├─ spec.md                    output of `news component <type>`
├─ index-line.txt             one line for `news components`
├─ fixtures/                  JSON fixtures: default, edge cases, real-data example
└─ TESTING.md                 written later by the Opus tester (sign-off)
```

## Contract every component follows

1. **Data references, not numbers.** Props take data IDs that resolve against the page's `data.json`; the component never receives a bare number from the model. Every rendered figure carries its source ID so `SourceRef` can show the citation popover.
2. **Missing data renders honestly**: a gap, "No data", or the component's documented empty state. Never a zero, never a crash.
3. **Server-rendered first.** The static HTML must be complete and readable with no JavaScript. Only the interactive part (tooltips, toggles, click-to-load) hydrates, and static components ship 0 KB of JS.
4. **Tokens only** in CSS (see `news-design-system`). No inline styles except computed geometry (bar widths, positions).
5. **Accessible by construction**: semantic elements (`figure`, `figcaption`, `table`, `dl`, `button`), accessible names, keyboard support for anything interactive, visible focus, nothing carried by colour alone. Charts get a text summary as their accessible name plus the "Show the data table" toggle.
6. **Responsive by re-layout**, not by scaling: phone first (360px), then wider. Text never renders below 12px. Horizontal bar charts and waterfalls are HTML/CSS; line and column charts are SVG sized for phone first.
7. **Formatting through the shared formatters** (numbers, currencies, dates, fiscal periods) so every component writes "$391.0bn", "−5.7%" and "1 November 2024" the same way.
8. **Wrapped in `FigureFrame`** if it is a chart or table: brand rule and tag, title, subtitle, legend, plot, source line, actions.

## Steps

1. Read the spec and the nearest existing component; reuse primitives (`FigureFrame`, `SourceRef`, `Rule`, `VisuallyHidden`, formatters) rather than re-implementing them.
2. Write the props interface and data shape first; record it in `spec.md` (props, data shape, one realistic example in OpenUI Lang).
3. Build the server-rendered markup, then styles, then the interactive island if any.
4. Create fixtures: `default.json`, a real-data example from a public source, and edge cases (empty, one item, maximum items, long labels, negatives, very large/small numbers, missing values).
5. Self-check quickly: render the fixtures at 360px and 1440px in light and dark, tab through it, run the unit tests you wrote for its logic. This is a sanity pass, not the formal test (the tester owns that).
6. Write `index-line.txt`: `ComponentName — purpose — data shape` in under 120 characters. The index sits in every page-writing prompt, so every character counts.
7. Commit on your branch (`phase-3/<family>/<component>`), then return to the orchestrator: branch, a 5-line summary, open questions. Hand off for testing; don't mark the component done yourself.

## Charts specifically

Follow `design/charts.md`: takeaway title, unit in the subtitle, bars from zero, one y-axis, solid horizontal hairline gridlines, direct labels for ≤ 4 series, value labels only where they matter, the highlight in crimson with a bold label, everything else recessive, no pies, no dual axes, no animation. Assign categorical colours by entity in slot order and never by rank.
