// Tester's formatting tests (news-component-testing, "Formatting" and "Data"): the shared formatters as they render
// inside SourceRef and its citation popover, one currency, scale, unit, period and locator at a time, against
// design/content-style.md. Synthetic data goes through the real schema (tests/support/data.ts).
import { screen, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";
import { type PointInput, syntheticResolver, TEST_SOURCE } from "../../../tests/support/data";
import { renderComponent } from "../../../tests/support/render";
import { PageDataProvider } from "../../renderer/data/context";
import type { PageDataResolver } from "../../renderer/data/resolve";
import { RefText } from "./RefText";
import { SourceRef } from "./SourceRef";

const MINUS = "−";

function renderRef(point: PointInput, props: Partial<Parameters<typeof SourceRef>[0]> = {}, extra = {}) {
  const resolver = syntheticResolver({ points: { dp_test_value: point }, ...extra });
  const utils = renderComponent(
    <PageDataProvider resolver={resolver}>
      <p>
        <SourceRef dataRef="dp_test_value" {...props} />
      </p>
    </PageDataProvider>,
  );
  const trigger = utils.container.querySelector("button.source-ref") as HTMLButtonElement | null;
  const panel = trigger ? (document.getElementById(trigger.getAttribute("popovertarget") ?? "") as HTMLElement) : null;
  return { ...utils, resolver, trigger, panel };
}

const shown = (point: PointInput, props: Partial<Parameters<typeof SourceRef>[0]> = {}) => {
  const { trigger, container, unmount } = renderRef(point, props);
  const text = trigger?.textContent ?? container.textContent ?? "";
  unmount();
  return text;
};

describe("SourceRef: money as the source states it (content-style.md, Currencies)", () => {
  it.each<[string, PointInput, string, string]>([
    ["US dollar", { value: "391035", currency: "USD", scale: "millions" }, "$391.0bn", "$391,035 million"],
    ["euro", { value: "1200", currency: "EUR", scale: "billions" }, "€1.2trn", "€1,200 billion"],
    ["pound", { value: "65.6", currency: "GBP", scale: "billions" }, "£65.6bn", "£65.6 billion"],
    ["rupee", { value: "2100000", currency: "INR", scale: "millions" }, "₹2.1trn", "₹2,100,000 million"],
    ["won", { value: "300870", currency: "KRW", scale: "billions" }, "₩300.9trn", "₩300,870 billion"],
    ["Taiwan dollar", { value: "2894", currency: "TWD", scale: "billions" }, "NT$2.9trn", "NT$2,894 billion"],
    ["Hong Kong dollar", { value: "82.3", currency: "HKD", scale: "billions" }, "HK$82.3bn", "HK$82.3 billion"],
    ["Australian dollar", { value: "5.4", currency: "AUD", scale: "billions" }, "A$5.4bn", "A$5.4 billion"],
    ["Canadian dollar", { value: "12", currency: "CAD", scale: "billions" }, "C$12bn", "C$12 billion"],
    ["Singapore dollar", { value: "420", currency: "SGD", scale: "millions" }, "S$420m", "S$420 million"],
    ["any other code", { value: "4.5", currency: "CHF", scale: "billions" }, "CHF 4.5bn", "CHF 4.5 billion"],
    [
      "yen, currency not named",
      { value: "1200", currency: "JPY", scale: "billions" },
      "1.2trn yen",
      "1,200 billion yen",
    ],
    ["yuan, currency not named", { value: "850", currency: "CNY", scale: "millions" }, "850m yuan", "850 million yuan"],
  ])("%s: compact on the page, the source's own digits in the popover", (_name, point, compact, asReported) => {
    const { trigger, panel } = renderRef(point);
    expect(trigger?.textContent).toBe(compact);
    expect(panel?.querySelector(".popover__value")?.textContent).toBe(asReported);
    expect(panel).toHaveAttribute("aria-label", `Source for ${compact}`);
  });

  it("uses ¥ for yen and yuan only when the subtitle names the currency", () => {
    expect(shown({ value: "1200", currency: "JPY", scale: "billions" }, { currencyNamed: true })).toBe("¥1.2trn");
    expect(shown({ value: "850", currency: "CNY", scale: "millions" }, { currencyNamed: true })).toBe("¥850m");
    // Other currencies are not affected by the flag.
    expect(shown({ value: "391035", currency: "USD", scale: "millions" }, { currencyNamed: true })).toBe("$391.0bn");
  });

  it("puts a true minus before the currency, never a hyphen", () => {
    const { trigger, panel } = renderRef({ value: "-5700", currency: "USD", scale: "millions" });
    expect(trigger?.textContent).toBe(`${MINUS}$5.7bn`);
    expect(panel?.querySelector(".popover__value")?.textContent).toBe(`${MINUS}$5,700 million`);
    expect(shown({ value: "-4.5", currency: "CHF", scale: "billions" })).toBe(`${MINUS}CHF 4.5bn`);
    expect(shown({ value: "-1200", currency: "JPY", scale: "billions" })).toBe(`${MINUS}1.2trn yen`);
  });

  it("never adds precision the source does not have, and rolls 999.96bn up to 1.0trn", () => {
    expect(shown({ value: "2", currency: "USD", scale: "billions" })).toBe("$2bn");
    expect(shown({ value: "999960", currency: "USD", scale: "millions" })).toBe("$1.0trn");
    expect(shown({ value: "2.45", currency: "USD", scale: "billions" })).toBe("$2.5bn"); // half away from zero
    expect(shown({ value: "-2.45", currency: "USD", scale: "billions" })).toBe(`${MINUS}$2.5bn`);
  });

  it("writes amounts under a million with thousands separators and the source's digits", () => {
    expect(shown({ value: "167045", currency: "USD", scale: "units" })).toBe("$167,045");
    expect(shown({ value: "0.5", currency: "GBP", scale: "units" })).toBe("£0.5");
  });

  it("handles zero, very large (at least 1e12) and very small money", () => {
    expect(shown({ value: "0", currency: "USD", scale: "millions" })).toBe("$0");
    expect(shown({ value: "1000000000000", currency: "USD", scale: "units" })).toBe("$1.0trn");
    expect(shown({ value: "123456789012345678", currency: "USD", scale: "units" })).toBe("$123,456.8trn");
    expect(shown({ value: "0.000001", currency: "USD", scale: "units" })).toBe("$0.000001");
  });
});

describe("SourceRef: units, percentages and points (content-style.md, Numbers)", () => {
  it.each<[string, PointInput, string]>([
    ["percentage", { value: "2.4", unit: "Percent" }, "2.4%"],
    ["negative percentage", { value: "-5.7", unit: "Percent" }, `${MINUS}5.7%`],
    ["zero percentage", { value: "0", unit: "Percent" }, "0%"],
    ["percentage keeps the source's digits", { value: "2.46", unit: "%" }, "2.46%"],
    ["a rate's trailing zero is kept", { value: "6.50", unit: "Percent" }, "6.50%"],
    ["a rate's trailing zero from the source's decimals", { value: "6.5", unit: "Percent", decimals: 2 }, "6.50%"],
    ["a small negative percentage is not rounded to zero", { value: "-0.04", unit: "Percent" }, `${MINUS}0.04%`],
    ["negative zero is unsigned", { value: "-0.0", unit: "Percent" }, "0.0%"],
    ["percentage points", { value: "2.1", unit: "percentage points" }, "2.1 points"],
    ["one point", { value: "1", unit: "pp" }, "1 point"],
    ["basis points", { value: "-25", unit: "basis points" }, `${MINUS}25 basis points`],
    ["a count with thousands separators", { value: "167045", unit: "Persons" }, "167,045"],
    ["millions of a count", { value: "420", unit: "Persons", scale: "millions" }, "420m"],
    ["a decimal", { value: "46.2", unit: "Index" }, "46.2"],
    ["very small", { value: "0.000000012", unit: "mol/L" }, "0.000000012"],
    ["very large count", { value: "8100000000000", unit: "Number" }, "8.1trn"],
  ])("%s", (_name, point, expected) => {
    expect(shown(point)).toBe(expected);
  });

  it("writes the unit verbatim in the popover for units the house style does not know", () => {
    const { panel } = renderRef({ value: "0.000000012", unit: "mol/L" });
    expect(panel?.querySelector(".popover__value")?.textContent).toBe("0.000000012 mol/L");
    const counted = renderRef({ value: "420", unit: "Persons", scale: "millions" });
    expect(counted.panel?.querySelector(".popover__value")?.textContent).toBe("420 million Persons");
  });

  it("never writes a hyphen-minus before a digit anywhere in the trigger or the popover", () => {
    const { container } = renderRef({ value: "-391035.5", currency: "USD", scale: "millions" });
    expect(container.textContent).not.toMatch(/-\d/);
    expect(container.textContent).toContain(MINUS);
  });
});

describe("SourceRef: the citation popover's content (components.md, Citation popover)", () => {
  it("shows the exact value, the label and period, the location, the document, the retrieval date and the link", () => {
    const { panel } = renderRef(
      {
        value: "391035",
        currency: "USD",
        scale: "millions",
        label: "Total net sales",
        period: "pd_fy2024",
        locator: { kind: "xbrl", tag: "us-gaap:Revenues", context: "FY2024" },
      },
      {},
      { periods: { pd_fy2024: { label: "fiscal 2024", fiscal: true } } },
    );
    if (!panel) throw new Error("no popover");
    expect(panel.querySelector(".popover__value")?.textContent).toBe("$391,035 million");
    expect([...panel.querySelectorAll(".popover__line")].map((e) => e.textContent)).toEqual([
      "Total net sales, fiscal 2024",
      TEST_SOURCE.title,
    ]);
    expect([...panel.querySelectorAll(".popover__meta")].map((e) => e.textContent)).toEqual([
      "XBRL tag us-gaap:Revenues",
      "Retrieved 1 November 2024",
    ]);
    const link = within(panel).getByRole("link", { name: "Open the source", hidden: true });
    expect(link).toHaveAttribute("href", TEST_SOURCE.url);
    // The icon in the link is decorative. (The closed native popover is hidden from the tree, hence hidden: true.)
    expect(link.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
  });

  it("writes fiscal and calendar period labels exactly as the source does", () => {
    for (const label of ["Q4 FY26", "fourth quarter of fiscal 2024", "first quarter of fiscal 2025", "Q3 2024"]) {
      const { panel, unmount } = renderRef(
        { value: "82.3", currency: "HKD", scale: "billions", label: "Revenue", period: "pd_p" },
        {},
        { periods: { pd_p: { label, fiscal: true } } },
      );
      expect(panel?.querySelector(".popover__line")?.textContent).toBe(`Revenue, ${label}`);
      unmount();
    }
  });

  it.each<[Record<string, unknown>, string | null]>([
    [{ kind: "xbrl", tag: "us-gaap:Revenues" }, "XBRL tag us-gaap:Revenues"],
    [{ kind: "pdf", page: 17, table: 2 }, "Page 17, table 2"],
    [{ kind: "pdf", page: 17 }, "Page 17"],
    [{ kind: "json", path: "$.x", series: "CPIAUCSL" }, "Series CPIAUCSL"],
    [{ kind: "sdmx", key: "M.US.CPI" }, "Series M.US.CPI"],
    [{ kind: "xbrl" }, null],
    [{ kind: "html", selector: "#t1" }, null],
  ])("describes the location %j", (locator, expected) => {
    const { panel } = renderRef({ value: "1", unit: "Number", locator });
    const metas = [...(panel?.querySelectorAll(".popover__meta") ?? [])].map((e) => e.textContent);
    if (expected) expect(metas[0]).toBe(expected);
    else expect(metas).toEqual(["Retrieved 1 November 2024"]);
  });

  it("shows a date-time retrieval as its UTC calendar date, day month year with no comma", () => {
    const { panel } = renderRef(
      { value: "1", unit: "Number", source: "src_late" },
      {},
      { sources: { src_late: { ...TEST_SOURCE, retrieved_at: "2026-11-13T23:30:00-05:00" } } },
    );
    expect(panel?.textContent).toContain("Retrieved 14 November 2026");
  });

  it("a derived value says it was calculated by News and cites every input's source", () => {
    const resolver = syntheticResolver({
      points: {
        dp_a: { value: "383285", currency: "USD", scale: "millions", source: "src_test" },
        dp_b: { value: "391035", currency: "USD", scale: "millions", source: "src_second" },
      },
      derived: {
        dv_growth: {
          label: "Change in net sales",
          value: "2.0",
          unit: "%",
          currency: null,
          scale: null,
          formula: "pct_change",
          inputs: ["dp_a", "dp_b"],
          rounding: 1,
        },
      },
      sources: { src_second: { ...TEST_SOURCE, title: "Second filing", url: "https://example.org/second" } },
    });
    const { container } = renderComponent(
      <PageDataProvider resolver={resolver}>
        <p>
          Up <SourceRef dataRef="dv_growth" />
        </p>
      </PageDataProvider>,
    );
    expect(screen.getByRole("button")).toHaveTextContent("2.0%");
    const panel = container.querySelector("[popover]") as HTMLElement;
    expect(panel.textContent).toContain("Calculated by News from the figures below.");
    expect(panel.querySelectorAll(".source-ref__source")).toHaveLength(2);
    expect(
      within(panel)
        .getAllByRole("link", { name: "Open the source", hidden: true })
        .map((a) => a.getAttribute("href")),
    ).toEqual([TEST_SOURCE.url, "https://example.org/second"]);
    expect([...resolver.usedSources].sort()).toEqual(["src_second", "src_test"]);
  });
});

describe("SourceRef: missing data (never a zero)", () => {
  it.each([
    ["a null value", { points: { dp_x: { value: null, unit: "Percent" } } }, "dp_x", "no-value"],
    ["an unknown reference", {}, "dp_missing", "unknown-ref"],
    ["a dv_ reference that does not exist", {}, "dv_missing", "unknown-ref"],
    ["a reference of the wrong kind", {}, "sr_series", "wrong-kind"],
  ])("%s says 'No data', is not interactive and reports the problem", (_n, data, ref, code) => {
    const resolver = syntheticResolver(data as never);
    const { container } = renderComponent(
      <PageDataProvider resolver={resolver}>
        <p>
          <SourceRef dataRef={ref} className="text-figure" />
        </p>
      </PageDataProvider>,
    );
    const el = container.querySelector(".source-ref--missing");
    expect(el).toHaveTextContent(/^No data$/);
    expect(el).toHaveAttribute("data-ref", ref);
    expect(el).toHaveClass("text-figure");
    expect(container.querySelector("button, [popover]")).toBeNull();
    expect(container.textContent).not.toMatch(/\d/);
    expect(resolver.problems.map((p) => p.code)).toContain(code);
  });
});

describe("RefText", () => {
  const resolver = (): PageDataResolver =>
    syntheticResolver({
      points: {
        dp_sales: { value: "391035", currency: "USD", scale: "millions" },
        dp_gap: { value: null, unit: "Percent" },
      },
      periods: { pd_fy2024: { label: "fiscal 2024", fiscal: true } },
    });
  const wrap = (ui: ReactNode, r = resolver()) => <PageDataProvider resolver={r}>{ui}</PageDataProvider>;

  it("cites values with a popover by default, and fills periods with their labels", () => {
    const { container } = renderComponent(wrap(<p>{<RefText text="Sales were {dp_sales} in {pd_fy2024}." />}</p>));
    expect(container.textContent?.replace(/\s+/g, " ")).toContain("Sales were $391.0bn");
    expect(screen.getByRole("button", { name: "$391.0bn" })).toHaveClass("source-ref");
    expect(container.querySelector("p")?.firstChild?.textContent).toBe("Sales were ");
  });

  it("says 'no data' in lower case inside a sentence for a missing value, and never writes a zero", () => {
    const r = resolver();
    const { container } = renderComponent(wrap(<p>{<RefText text="Growth was {dp_gap}." cite="plain" />}</p>, r));
    expect(container.textContent).toBe("Growth was no data.");
    expect(container.querySelector(".source-ref--missing")).toHaveAttribute("data-ref", "dp_gap");
  });

  it("text with no tokens and an empty string render as themselves", () => {
    const { container } = renderComponent(
      wrap(
        <>
          <p>
            <RefText text="No figures here" />
          </p>
          <p>
            <RefText text="" />
          </p>
        </>,
      ),
    );
    expect([...container.querySelectorAll("p")].map((p) => p.textContent)).toEqual(["No figures here", ""]);
  });
});
