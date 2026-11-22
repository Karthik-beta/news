// Server rendering of the primitives in plain Bun (news-component-testing, "Server rendering"): every fixture case
// renders with no Vite or Astro in between, the HTML is complete without JavaScript (no script, no hydration
// marker, no empty output), static primitives carry no inline style, and render time stays within budget.
import { describe, expect, test } from "bun:test";
import { Fragment, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { cases as button } from "../../src/components/Button/fixtures/cases";
import { cases as checkbox } from "../../src/components/Checkbox/fixtures/cases";
import type { FixtureCase } from "../../src/components/contract";
import { cases as figureFrame } from "../../src/components/FigureFrame/fixtures/cases";
import { cases as icon } from "../../src/components/Icon/fixtures/cases";
import { cases as loadingRegion } from "../../src/components/LoadingRegion/fixtures/cases";
import { cases as notFound } from "../../src/components/NotFound/fixtures/cases";
import { cases as popover } from "../../src/components/Popover/fixtures/cases";
import { cases as reportForm } from "../../src/components/ReportForm/fixtures/cases";
import { cases as rule } from "../../src/components/Rule/fixtures/cases";
import { cases as searchField } from "../../src/components/SearchField/fixtures/cases";
import { cases as select } from "../../src/components/Select/fixtures/cases";
import { cases as sourceRef } from "../../src/components/SourceRef/fixtures/cases";
import { cases as stateMessage } from "../../src/components/StateMessage/fixtures/cases";
import { cases as text } from "../../src/components/Text/fixtures/cases";
import { cases as textArea } from "../../src/components/TextArea/fixtures/cases";
import { cases as textField } from "../../src/components/TextField/fixtures/cases";
import { cases as visuallyHidden } from "../../src/components/VisuallyHidden/fixtures/cases";
import { PageData, PageSources } from "../../src/renderer/bundle/schema";
import { PageDataProvider } from "../../src/renderer/data/context";
import { PageDataResolver } from "../../src/renderer/data/resolve";
import { mergeBundleData } from "../../src/renderer/fixtures/bundles/assemble";
import { readFixtureBundles } from "../../src/renderer/fixtures/bundles/fs";

const GALLERIES: Record<string, FixtureCase[]> = {
  Button: button,
  Checkbox: checkbox,
  LoadingRegion: loadingRegion,
  NotFound: notFound,
  ReportForm: reportForm,
  SearchField: searchField,
  Select: select,
  StateMessage: stateMessage,
  TextArea: textArea,
  TextField: textField,
  FigureFrame: figureFrame,
  Icon: icon,
  Popover: popover,
  Rule: rule,
  SourceRef: sourceRef,
  Text: text,
  VisuallyHidden: visuallyHidden,
};

const bundles = readFixtureBundles();
const merged = mergeBundleData(bundles["apple-fy2024"], bundles["edge-cases"]);
const resolver = () => new PageDataResolver(PageData.parse(merged.data), PageSources.parse(merged.sources));
const render = (ui: ReactNode) => renderToStaticMarkup(<PageDataProvider resolver={resolver()}>{ui}</PageDataProvider>);

describe("primitives under Bun", () => {
  for (const [name, cases] of Object.entries(GALLERIES)) {
    for (const c of cases) {
      test(`${name}: "${c.name}" renders complete static HTML with no React warnings`, () => {
        const warnings: string[] = [];
        const original = console.error;
        console.error = (...args: unknown[]) => warnings.push(args.map(String).join(" "));
        let html = "";
        try {
          html = render(c.render());
        } finally {
          console.error = original;
        }
        expect(warnings).toEqual([]);
        expect(html.length).toBeGreaterThan(0);
        expect(html).not.toContain("<script");
        expect(html).not.toMatch(/<!--\$|data-reactroot|__react/); // no hydration or suspense markers
        expect(html).not.toMatch(/\son[a-z]+=/i); // no inline event handlers
        // Only the Tooltip (computed geometry for islands) may carry an inline style.
        if (!/tooltip/.test(c.name)) expect(html).not.toContain("style=");
        // Numbers never arrive with a hyphen-minus.
        expect(html.replace(/<[^>]+>/g, "")).not.toMatch(/(^|[\s(])-\d/);
      });
    }
  }

  test("SourceRef's cited values are in the HTML as text, with the full popover, before any script runs", () => {
    const html = render(sourceRef[0]?.render());
    expect(html).toContain(">$391.0bn</button>");
    expect(html).toContain("$391,035 million");
    expect(html).toContain("Retrieved");
    expect(html).toMatch(/href="https:\/\/www\.sec\.gov\//);
  });

  test("ids from useId are stable between two renders of the same tree (no hydration mismatch risk)", () => {
    const ui = sourceRef[0]?.render();
    expect(render(ui)).toBe(render(ui));
  });

  test("render time: every gallery renders 50 times with p95 under 20 ms", () => {
    const report: string[] = [];
    for (const [name, cases] of Object.entries(GALLERIES)) {
      const times: number[] = [];
      for (let i = 0; i < 50; i++) {
        const start = performance.now();
        render(cases.map((c) => <Fragment key={c.name}>{c.render()}</Fragment>));
        times.push(performance.now() - start);
      }
      times.sort((a, b) => a - b);
      const p95 = times[Math.floor(times.length * 0.95)] as number;
      report.push(`${name} p95 ${p95.toFixed(2)} ms`);
      expect(p95).toBeLessThan(20);
    }
    console.log(report.join(", "));
  });
});
