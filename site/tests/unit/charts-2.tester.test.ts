// Component tester's checks for Charts II (#13, part 1), on top of the builder's charts-2.test.ts: Apple's fiscal
// 2024 statement figures in the fixtures equal SEC's XBRL facts; waterfall edge cases (lines that cannot be placed,
// a missing first total, a loss, a total below zero, very large and very small values, the row limits); Sankey
// sums, the 2px minimum under extreme ratios, folding, every reason it cannot flow, and the CSS switch-over widths;
// the "fell" rule of every chart; stacked and segmented bars, dot plots and slopes with gaps, negatives, ties,
// long lists and other units; and labels that never overlap in the layouts.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { type DotPlotProps, dotLegend, dotPlot, dotPlotData } from "../../src/components/DotPlot/DotPlot";
import { SANKEY, type SankeyLineInput, sankeyGraph, sankeyLayout, wrapLines } from "../../src/components/Sankey/layout";
import { SANKEY_MAX_COLUMNS, type SankeyProps, sankeyData, sankeyPlot } from "../../src/components/Sankey/Sankey";
import SankeySpec from "../../src/components/Sankey/spec";
import { type SegmentedBarProps, segmentedBarData } from "../../src/components/SegmentedBar/SegmentedBar";
import SegmentedBarSpec from "../../src/components/SegmentedBar/spec";
import { type SlopeProps, slopeData, slopePlot } from "../../src/components/Slope/Slope";
import SlopeSpec from "../../src/components/Slope/spec";
import {
  type StackedBarProps,
  stackedBarData,
  stackLegend,
  stackPlot,
} from "../../src/components/StackedBar/StackedBar";
import StackedBarSpec from "../../src/components/StackedBar/spec";
import WaterfallSpec from "../../src/components/Waterfall/spec";
import { addExact, compareExact, sumExact, waterfallSteps } from "../../src/components/Waterfall/steps";
import {
  mismatchNote,
  type WaterfallProps,
  type WaterfallRowInput,
  waterfallData,
  waterfallPlot,
} from "../../src/components/Waterfall/Waterfall";
import { PageData, PageSources } from "../../src/renderer/bundle/schema";
import { PageDataResolver } from "../../src/renderer/data/resolve";
import { componentFixtureData } from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { renderPage } from "../../src/renderer/render";
import { siteRoot } from "../support/tokens";

// ---------------------------------------------------------------------------------------------------------------
// Synthetic data: the component fixtures' data plus points made for one test.

interface Point {
  value: string | null;
  unit?: string;
  currency?: string | null;
  scale?: string | null;
}

function bundleWith(points: Record<string, Point> = {}) {
  const base = structuredClone(componentFixtureData()) as {
    data: { points: Record<string, unknown> };
    sources: unknown;
  };
  for (const [ref, p] of Object.entries(points)) {
    base.data.points[ref] = {
      label: ref,
      value: p.value,
      value_text: p.value,
      unit: p.unit ?? "USD",
      currency: p.currency === undefined ? "USD" : p.currency,
      scale: p.scale === undefined ? "millions" : p.scale,
      period: "pd_fx_2024",
      source: "src_fixture_synthetic",
      locator: { kind: "json", series: ref },
    };
  }
  return base;
}

function resolverWith(points: Record<string, Point> = {}): PageDataResolver {
  const base = bundleWith(points);
  return new PageDataResolver(PageData.parse(base.data), PageSources.parse(base.sources));
}

/** A statement from rows written as [label, kind, value]; each value becomes a point dp_t_<index>. */
function statement(rows: [string, WaterfallRowInput["kind"], string | null][], extra: Record<string, Point> = {}) {
  const points: Record<string, Point> = { ...extra };
  const input = rows.map(([label, kind, value], i) => {
    const ref = `dp_t_${i}`;
    points[ref] = { value };
    return { label, kind, value: ref };
  });
  return { rows: input, data: resolverWith(points) };
}

const wf = (rows: WaterfallRowInput[], data: PageDataResolver) => {
  const props: WaterfallProps = { title: "T", subtitle: "S", rows };
  const chart = waterfallData(props, data);
  return { chart, plot: waterfallPlot(chart, data) };
};

// ---------------------------------------------------------------------------------------------------------------

describe("Apple's fiscal 2024 statement figures equal SEC's XBRL facts (data fidelity)", () => {
  const sec = JSON.parse(readFileSync(join(siteRoot, "tests/support/sec/aapl-fy2024-companyfacts.json"), "utf8")) as {
    accn: string;
    facts: Record<string, { val: number; end: string; form: string; filed: string }>;
  };
  const bundles = ["charts", "apple-fy2024"].map((b) => ({
    b,
    data: JSON.parse(readFileSync(join(siteRoot, `src/renderer/fixtures/bundles/${b}/data.json`), "utf8")) as {
      points: Record<
        string,
        {
          value: string;
          value_text?: string;
          scale: string;
          period: string;
          source: string;
          locator: Record<string, string>;
        }
      >;
    },
    sources: JSON.parse(readFileSync(join(siteRoot, `src/renderer/fixtures/bundles/${b}/sources.json`), "utf8")) as {
      sources: Record<string, { url: string; published_at?: string }>;
    },
  }));
  /** Every FY2024 Apple point with an undimensioned XBRL locator ("FY2024"): the statement's own lines. */
  const statementPoints = bundles.flatMap(({ b, data }) =>
    Object.entries(data.points)
      .filter(([ref, p]) => ref.startsWith("dp_aapl_") && p.locator.kind === "xbrl" && p.locator.context === "FY2024")
      .map(([ref, p]) => ({ b, ref, p })),
  );

  it("checks every builder-typed line of the statement of operations", () => {
    const refs = new Set(statementPoints.map((s) => s.ref));
    for (const ref of [
      "dp_aapl_net_sales_fy2024",
      "dp_aapl_cost_of_sales_fy2024",
      "dp_aapl_gross_margin_fy2024",
      "dp_aapl_rnd_fy2024",
      "dp_aapl_sga_fy2024",
      "dp_aapl_operating_income_fy2024",
      "dp_aapl_other_income_fy2024",
      "dp_aapl_pretax_income_fy2024",
      "dp_aapl_tax_fy2024",
      "dp_aapl_net_income_fy2024",
    ]) {
      expect(refs.has(ref), ref).toBe(true);
    }
  });

  it.each(statementPoints.map((s) => [`${s.b} ${s.ref}`, s] as const))(
    "%s: value, tag, filing and period match SEC",
    (_name, { p }) => {
      const fact = sec.facts[p.locator.tag as string];
      expect(fact, `${p.locator.tag} in SEC companyfacts`).toBeDefined();
      if (!fact) return;
      expect(p.locator.accn).toBe(sec.accn);
      expect(fact.end).toBe("2024-09-28");
      expect(fact.form).toBe("10-K");
      expect(p.scale).toBe("millions");
      // SEC serves whole dollars; the fixture states millions as published. Compared exactly, as decimals.
      expect(`${p.value}000000`).toBe(String(fact.val));
      if (p.value_text) expect(p.value_text.replace(/,/g, "")).toBe(p.value);
      expect(p.period).toBe("pd_fy2024");
    },
  );

  it("the 10-K source points at the same filing, published the day SEC received it", () => {
    for (const { sources } of bundles) {
      const src = sources.sources.src_aapl_10k_2024;
      expect(src?.url).toContain(`/${sec.accn.replace(/-/g, "")}/`);
      expect(src?.published_at).toBe(sec.facts["us-gaap:NetIncomeLoss"]?.filed);
    }
  });

  it("the dimensioned lines (categories, products, services, regions) add up to SEC's total net sales", () => {
    const pts = bundles[0]?.data.points ?? {};
    const v = (ref: string) => pts[ref]?.value as string;
    const total = String((sec.facts["us-gaap:RevenueFromContractWithCustomerExcludingAssessedTax"]?.val ?? 0) / 1e6);
    const categories = ["iphone", "mac", "ipad", "wearables"].map((c) => v(`dp_aapl_sales_${c}_fy2024`));
    expect(sumExact(categories)).toBe(v("dp_aapl_sales_products_fy2024"));
    expect(addExact(v("dp_aapl_sales_products_fy2024"), v("dp_aapl_sales_services_fy2024"))).toBe(total);
    const regions = ["americas", "europe", "china", "japan", "rapac"].map((r) => v(`dp_aapl_sales_${r}_fy2024`));
    expect(sumExact(regions)).toBe(total);
  });
});

// ---------------------------------------------------------------------------------------------------------------

describe("Waterfall edge cases", () => {
  it("after a line with no value, the lines up to the next total are hatched with their values, then placing resumes", () => {
    const { rows, data } = statement([
      ["Revenue", "total", "1000"],
      ["Cost", "deduct", "600"],
      ["Restructuring", "deduct", null],
      ["Opex", "deduct", "150"],
      ["Other", "add", "25"],
      ["Operating profit", "total", "275"],
      ["Tax", "deduct", "75"],
      ["Net profit", "total", "200"],
    ]);
    const { chart, plot } = wf(rows, data);
    const placed = plot.rows.map((r) => r.start !== null);
    expect(placed).toEqual([true, true, false, false, false, true, true, true]);
    expect(plot.rows.map((r) => r.value)).toEqual([
      "$1.0bn",
      "−$600m",
      "No data",
      "−$150m",
      "+$25m",
      "$275m",
      "−$75m",
      "$200m",
    ]);
    // An unplaced line's label still needs room past the track.
    expect(Number.parseInt(plot.room.positive, 10)).toBeGreaterThanOrEqual("No data".length + 1);
    // A total after an unknown running amount cannot be checked, so there is no mismatch note.
    expect(chart.mismatches).toEqual([]);
    expect(mismatchNote(chart)).toBeNull();
    // The data table keeps every line, the missing one as a gap.
    expect(chart.model?.rows.map((r) => r.cells[0]?.value)).toEqual([
      "1000",
      "600",
      null,
      "150",
      "25",
      "275",
      "75",
      "200",
    ]);
  });

  it("a first total with no value leaves the lines until the next total unplaced", () => {
    const { rows, data } = statement([
      ["Revenue", "total", null],
      ["Cost", "deduct", "600"],
      ["Gross profit", "total", "400"],
      ["Opex", "deduct", "100"],
      ["Operating profit", "total", "300"],
    ]);
    const { plot } = wf(rows, data);
    expect(plot.rows.map((r) => r.start !== null)).toEqual([false, false, true, true, true]);
  });

  it("names every total the lines do not add up to, in order", () => {
    const { rows, data } = statement([
      ["Revenue", "total", "1000"],
      ["Cost", "deduct", "600"],
      ["Gross profit", "total", "390"],
      ["Opex", "deduct", "100"],
      ["Operating profit", "total", "300"],
    ]);
    const { chart } = wf(rows, data);
    expect(chart.mismatches.map((m) => [m.index, m.running, m.reported])).toEqual([
      [2, "400", "390"],
      [4, "290", "300"],
    ]);
    expect(mismatchNote(chart)).toBe(
      "The lines shown do not add up to Gross profit or Operating profit: the source has lines not shown here.",
    );
  });

  it("a loss: the result takes the neutral highlight, its label sits left of its bar, and room is made on the left", () => {
    const { rows, data } = statement([
      ["Revenue", "total", "100"],
      ["Cost", "deduct", "180"],
      ["Net loss", "total", "-80"],
    ]);
    const { chart, plot } = wf(rows, data);
    expect(chart.fell).toBe(true);
    const net = plot.rows[2];
    expect(net).toMatchObject({ role: "highlight-neutral", negative: true, value: "−$80m", highlight: true });
    // The deduction runs below zero: it floats from 100 to −80 and is labelled with its effect.
    expect(plot.rows[1]).toMatchObject({ value: "−$180m", role: "neutral" });
    expect(plot.zero).toBeGreaterThan(0);
    expect(plot.room.negative).toBe(`${"−$80m".length + 1}ch`);
    expect(chart.model?.rows[2]).toMatchObject({ highlight: true, fell: true });
  });

  it("break-even (a result of zero) and a profit stay crimson: neither fell", () => {
    for (const net of ["0", "1"]) {
      const { rows, data } = statement([
        ["Revenue", "total", "10"],
        ["Cost", "deduct", net === "0" ? "10" : "9"],
        ["Net", "total", net],
      ]);
      expect(wf(rows, data).plot.rows[2]?.role, net).toBe("highlight");
    }
  });

  it("keeps exact sums for very large (≥ 1e12) and very small values", () => {
    const big = statement(
      [
        ["Revenue", "total", "1234567890123.45"],
        ["Cost", "deduct", "234567890123.44"],
        ["Profit", "total", "1000000000000.01"],
      ],
      {},
    );
    expect(wf(big.rows, big.data).chart.mismatches).toEqual([]);
    expect(
      waterfallSteps([
        { kind: "total", value: "0.0000001" },
        { kind: "deduct", value: "0.00000003" },
        { kind: "total", value: "0.00000007" },
      ]).map((s) => s.end),
    ).toEqual(["0.0000001", "0.00000007", "0.00000007"]);
    expect(compareExact("0.30", addExact("0.1", "0.2"))).toBe(0);
  });

  it("a line in another unit or scale is left out, never converted, and the rest still add up", () => {
    const { rows, data } = statement([
      ["Revenue", "total", "1000"],
      ["Cost", "deduct", "600"],
      ["Gross profit", "total", "400"],
    ]);
    const odd = resolverWith({
      dp_t_0: { value: "1000" },
      dp_t_1: { value: "600" },
      dp_t_2: { value: "400" },
      dp_t_x: { value: "5", scale: "billions" },
      dp_t_y: { value: "5", currency: "EUR" },
    });
    const withOdd = [
      ...rows.slice(0, 2),
      { label: "Billions", kind: "deduct" as const, value: "dp_t_x" },
      { label: "Euros", kind: "deduct" as const, value: "dp_t_y" },
      rows[2] as WaterfallRowInput,
    ];
    const { chart } = wf(withOdd, odd);
    expect(chart.lines.map((l) => l.label)).toEqual(["Revenue", "Cost", "Gross profit"]);
    expect(chart.mismatches).toEqual([]);
    expect(wf(rows, data).chart.lines).toHaveLength(3);
  });

  it("the row schema: 2–16 rows, the first a total", () => {
    const row = (kind: string) => ({ label: "L", value: "dp_x", kind });
    const ok = (rows: unknown[]) => WaterfallSpec.props.safeParse({ title: "T", subtitle: "S", rows }).success;
    expect(ok([row("total"), row("deduct")])).toBe(true);
    expect(ok([row("total")])).toBe(false);
    expect(ok([row("deduct"), row("total")])).toBe(false);
    expect(ok(Array.from({ length: 16 }, (_, i) => row(i === 0 ? "total" : "deduct")))).toBe(true);
    expect(ok(Array.from({ length: 17 }, (_, i) => row(i === 0 ? "total" : "deduct")))).toBe(false);
    expect(ok([row("total"), row("subtract")])).toBe(false);
  });
});

// ---------------------------------------------------------------------------------------------------------------

const line = (label: string, kind: SankeyLineInput["kind"], value: string | null): SankeyLineInput => ({
  label,
  ref: `dp_${label.toLowerCase().replace(/[^a-z0-9]+/g, "_")}`,
  kind,
  value,
  shown: value ?? "",
});

describe("Sankey edge cases", () => {
  it("says why each kind of statement cannot flow, so the waterfall shows at every width", () => {
    const reason = (lines: SankeyLineInput[]) => {
      const r = sankeyGraph(lines);
      return r.ok ? "ok" : r.reason;
    };
    const base = [line("Revenue", "total", "100"), line("Cost", "deduct", "60"), line("Profit", "total", "40")];
    expect(reason(base)).toBe("ok");
    expect(reason([line("Revenue", "total", "100"), line("Cost", "deduct", null), line("Profit", "total", "40")])).toBe(
      "missing",
    );
    expect(reason([line("Revenue", "total", "100"), line("Cost", "deduct", "120"), line("Loss", "total", "-20")])).toBe(
      "not-positive",
    );
    expect(reason([line("Revenue", "total", "100"), line("Cost", "deduct", "100"), line("Nil", "total", "0")])).toBe(
      "not-positive",
    );
    expect(reason([line("Revenue", "total", "100"), line("Cost", "deduct", "60"), line("Profit", "total", "41")])).toBe(
      "mismatch",
    );
    expect(reason([line("A", "segment", "70"), line("B", "segment", "31"), ...base])).toBe("mismatch");
    expect(reason([line("A", "segment", "110"), line("B", "segment", "-10"), ...base])).toBe("negative-segment");
    expect(reason([...base, line("Late gain", "add", "5")])).toBe("trailing-addition");
  });

  it("other expense (an addition published negative) leaves as a cost; other income joins the next total from beside the current one", () => {
    const g = sankeyGraph([
      line("Revenue", "total", "100"),
      line("Other expense", "add", "-10"),
      line("Interest income", "add", "5"),
      line("Profit", "total", "95"),
    ]);
    if (!g.ok) throw new Error(g.reason);
    const by = (label: string) => g.nodes.findIndex((n) => n.label === label);
    expect(g.nodes[by("Other expense")]).toMatchObject({ role: "neutral", column: 1 });
    expect(g.nodes[by("Interest income")]).toMatchObject({ role: "context", column: 0 });
    expect(g.links.find((l) => l.target === by("Other expense"))?.value).toBe("10");
    expect(g.links.find((l) => l.source === by("Interest income"))?.target).toBe(by("Profit"));
    expect(g.links.find((l) => l.source === by("Revenue") && l.target === by("Profit"))?.value).toBe("90");
  });

  it("a zero segment or zero cost draws no node and no flow, but stays in the data table", () => {
    const g = sankeyGraph([
      line("A", "segment", "100"),
      line("Z", "segment", "0"),
      line("Revenue", "total", "100"),
      line("Nil cost", "deduct", "0"),
      line("Cost", "deduct", "40"),
      line("Profit", "total", "60"),
    ]);
    if (!g.ok) throw new Error(g.reason);
    expect(g.nodes.map((n) => n.label)).not.toContain("Z");
    expect(g.nodes.map((n) => n.label)).not.toContain("Nil cost");
  });

  it("no flow is thinner than 2 units at extreme ratios, and the plot holds every node", () => {
    const tiny = [
      line("Revenue", "total", "1000000000000"),
      line("Huge cost", "deduct", "999999999999"),
      line("Profit", "total", "1"),
    ];
    const r = sankeyLayout(tiny);
    if (!r.ok) throw new Error(r.reason);
    for (const l of r.layout.links) expect(l.thickness).toBeGreaterThanOrEqual(SANKEY.minThickness);
    for (const n of r.layout.nodes) expect(n.y + n.height).toBeLessThanOrEqual(r.layout.height + 1e-6);

    // Twelve segments, every one under 1%, but one: eleven fold into one "Other".
    const segs = Array.from({ length: 11 }, (_, i) => line(`Seg ${i + 1}`, "segment", "1"));
    const many = sankeyLayout([
      line("Main", "segment", "989"),
      ...segs,
      line("Revenue", "total", "1000"),
      line("Cost", "deduct", "400"),
      line("Profit", "total", "600"),
    ]);
    if (!many.ok) throw new Error(many.reason);
    const others = many.layout.nodes.filter((n) => n.folded);
    expect(others).toHaveLength(1);
    expect(others[0]?.items).toHaveLength(11);
    for (const l of many.layout.links) expect(l.thickness).toBeGreaterThanOrEqual(SANKEY.minThickness);
  });

  it("labels never overlap in a column, at every node count, and the plot grows to fit them", () => {
    const segs = Array.from({ length: 12 }, (_, i) =>
      line(`Segment with a long name number ${i + 1}`, "segment", String(100 + i)),
    );
    const revenue = String(segs.reduce((s, x) => s + Number(x.value), 0));
    const r = sankeyLayout([
      ...segs,
      line("Revenue", "total", revenue),
      line("Cost", "deduct", "1000"),
      line("Profit", "total", String(Number(revenue) - 1000)),
    ]);
    if (!r.ok) throw new Error(r.reason);
    expect(r.layout.height).toBeGreaterThan(SANKEY.height);
    const byColumn = new Map<number, typeof r.layout.nodes>();
    for (const n of r.layout.nodes) byColumn.set(n.column, [...(byColumn.get(n.column) ?? []), n]);
    for (const nodes of byColumn.values()) {
      const sorted = [...nodes].sort((a, b) => a.labelTop - b.labelTop);
      for (let i = 1; i < sorted.length; i++) {
        const prev = sorted[i - 1] as (typeof sorted)[number];
        const h = (wrapLines(prev.label) + (prev.folded ? 0 : 1)) * SANKEY.lineHeight;
        expect((sorted[i] as (typeof sorted)[number]).labelTop, `${prev.label}`).toBeGreaterThanOrEqual(
          prev.labelTop + h - 1e-6,
        );
      }
      for (const n of nodes) expect(n.labelTop).toBeGreaterThanOrEqual(0);
    }
  });

  it("wraps names the way the browser does at 12 characters a line, a long word over several", () => {
    expect(wrapLines("Net income")).toBe(1);
    expect(wrapLines("Selling, general and administrative")).toBe(4);
    expect(wrapLines("Other income/(expense), net")).toBe(4);
    expect(wrapLines("Supercalifragilistic")).toBe(2);
  });

  it("the CSS switch-over widths are 7em for each gutter plus 7.5em per gap, for 2 to 10 columns", () => {
    const css = readFileSync(join(siteRoot, "src/components/Sankey/Sankey.css"), "utf8");
    for (let c = 2; c <= SANKEY_MAX_COLUMNS; c++) {
      const need = 14 + 7.5 * (c - 1);
      const block = new RegExp(
        `@container sankey \\(min-width: ${need}em\\)\\s*\\{\\s*\\.sankey--c${c} \\.sankey__flows`,
      );
      expect(css, `${c} columns at ${need}em`).toMatch(block);
    }
    expect(css).toMatch(/\.sankey__area\s*\{[^}]*inset-inline: 7em/);
    expect(css).toMatch(/\.sankey__label\s*\{[^}]*max-width: 7em/);
  });

  it("the widest statement the schema allows (10 columns) gets its own switch-over width, never a narrower one", () => {
    // 16 rows: a total, then seven deduction-and-total pairs, then a cost after the result; plus segments.
    const rows: [string, WaterfallRowInput["kind"], string][] = [["Revenue", "total", "1000"]];
    let running = 1000;
    for (let i = 0; i < 7; i++) {
      running -= 10;
      rows.push([`Cost ${i}`, "deduct", "10"], [`Total ${i}`, "total", String(running)]);
    }
    rows.push(["Late cost", "deduct", "10"]);
    const { rows: input } = statement(rows);
    const points: Record<string, Point> = { dp_s_a: { value: "600" }, dp_s_b: { value: "400" } };
    rows.forEach(([, , value], i) => {
      points[`dp_t_${i}`] = { value };
    });
    const seg = [
      { label: "A", value: "dp_s_a" },
      { label: "B", value: "dp_s_b" },
    ];
    const chart = sankeyData({ title: "T", subtitle: "S", rows: input, segments: seg }, resolverWith(points));
    expect(chart.layout?.ok && chart.layout.layout.columns).toBe(SANKEY_MAX_COLUMNS);
    const oui = `root = Page([wide])
wide = Sankey("T", "S", [${input
      .map((r) => `{label: "${r.label}", value: "${r.value}", kind: "${r.kind}"}`)
      .join(", ")}], [{label: "A", value: "dp_s_a"}, {label: "B", value: "dp_s_b"}])`;
    const html = renderPage(
      { oui, ...bundleWith(points), head: { flyTitle: "F", headline: "H", summary: "S" } } as never,
      { library: fixtureLibrary() },
    ).article;
    expect(html).toContain("sankey sankey--c10");
    expect(html).toContain('data-island="SankeyPlot"');
  });

  it("the schema caps segments at 12 and shares the waterfall's rows", () => {
    const rows = [
      { label: "Revenue", value: "dp_x", kind: "total" },
      { label: "Cost", value: "dp_y", kind: "deduct" },
    ];
    const seg = { label: "S", value: "dp_s" };
    const ok = (segments: unknown) => SankeySpec.props.safeParse({ title: "T", subtitle: "S", rows, segments }).success;
    expect(ok(Array.from({ length: 12 }, () => seg))).toBe(true);
    expect(ok(Array.from({ length: 13 }, () => seg))).toBe(false);
    expect(ok(null)).toBe(true);
  });

  it("the plot's stops run column by column, top to bottom, and a folded node writes no number", () => {
    const { rows, data } = statement(
      [
        ["Revenue", "total", "1000"],
        ["Cost", "deduct", "600"],
        ["Fee", "deduct", "4"],
        ["Fine", "deduct", "3"],
        ["Profit", "total", "393"],
      ],
      { dp_s_a: { value: "990" }, dp_s_b: { value: "6" }, dp_s_c: { value: "4" } },
    );
    const props: SankeyProps = {
      title: "T",
      subtitle: "S",
      rows,
      segments: [
        { label: "A", value: "dp_s_a" },
        { label: "B", value: "dp_s_b" },
        { label: "C", value: "dp_s_c" },
      ],
    };
    const chart = sankeyData(props, data);
    const plot = sankeyPlot(chart, data);
    if (!plot) throw new Error("must flow");
    const xs = plot.nodes.map((n) => n.x);
    expect([...xs].sort((a, b) => a - b)).toEqual(xs);
    const other = plot.nodes.filter((n) => n.label === "Other");
    expect(other).toHaveLength(2);
    for (const o of other) {
      expect(o.value).toBeNull();
      expect(o.tooltip.every((t) => t.series !== null && t.value !== null)).toBe(true);
    }
    expect(plot.flows.every((f) => f.owner >= 0 && f.owner < plot.nodes.length)).toBe(true);
  });
});

// ---------------------------------------------------------------------------------------------------------------

describe("StackedBar and SegmentedBar", () => {
  const data = resolverWith({
    dp_a1: { value: "50", unit: "%", currency: null, scale: null },
    dp_b1: { value: "30", unit: "%", currency: null, scale: null },
    dp_c1: { value: "20", unit: "%", currency: null, scale: null },
    dp_a2: { value: "60", unit: "%", currency: null, scale: null },
    dp_b2: { value: "-5", unit: "%", currency: null, scale: null },
    dp_c2: { value: "45", unit: "%", currency: null, scale: null },
    dp_a3: { value: "40", unit: "%", currency: null, scale: null },
    dp_b3: { value: null, unit: "%", currency: null, scale: null },
    dp_c3: { value: "60", unit: "%", currency: null, scale: null },
    dp_t3: { value: "120", unit: "%", currency: null, scale: null },
  });
  const props = (highlight: string | null, rows = [1, 2, 3]): StackedBarProps => ({
    title: "T",
    subtitle: "S",
    parts: ["A", "B", "C"],
    rows: rows.map((i) => ({
      label: `Row ${i}`,
      values: [`dp_a${i}`, `dp_b${i}`, `dp_c${i}`],
      total: i === 3 ? "dp_t3" : null,
    })),
    highlight,
  });

  it("the highlighted part fell when its last-row value is below its first-row value (gaps skipped)", () => {
    expect(stackedBarData(props("A"), data).roles).toEqual(["highlight-neutral", "context", "context"]);
    expect(stackedBarData(props("C"), data).roles).toEqual(["context", "context", "highlight"]);
    // B: 30, −5, gap → the last present value (−5) is below the first (30).
    expect(stackedBarData(props("B"), data).roles[1]).toBe("highlight-neutral");
    // One row: nothing to compare, never fell.
    expect(stackedBarData(props("A", [1]), data).roles[0]).toBe("highlight");
  });

  it("no highlight, or one not among the parts: the categorical colours in order", () => {
    expect(stackedBarData(props(null), data).roles).toEqual(["cat-1", "cat-2", "cat-3"]);
    expect(stackedBarData(props("Z"), data).roles).toEqual(["cat-1", "cat-2", "cat-3"]);
  });

  it("a negative part is left out of the bar but kept in the tooltip and the table; a gap is No data", () => {
    const chart = stackedBarData(props(null), data);
    const plot = stackPlot(chart, "stacked", data);
    expect(plot.rows[1]?.segments.map((s) => s.key)).toEqual(["1-0", "1-2"]);
    expect(plot.stops[1]?.tooltip.map((t) => t.value)).toEqual(["60%", "−5%", "45%"]);
    expect(plot.stops[2]?.tooltip.map((t) => t.value)).toEqual(["40%", null, "60%"]);
    // The table keeps the negative part (and a Total column, since one row publishes a total).
    expect(chart.model?.rows[1]?.cells.map((c) => c.value)).toEqual(["60", "-5", "45", null]);
  });

  it("every row is on one scale; a published total sits past the longer of the bar and itself", () => {
    const plot = stackPlot(stackedBarData(props(null), data), "stacked", data);
    // Largest of the bar sums (100, 105, 100) and the total (120) is the scale.
    const ends = plot.rows.map((r) => r.segments.reduce((s, x) => s + x.width, 0));
    expect(ends.map((e) => Math.round(e * 100) / 100)).toEqual([83.33, 87.5, 83.33]);
    expect(plot.rows[2]?.total).toMatchObject({ left: 100, value: "120%" });
    expect(plot.room).toBe("5ch");
  });

  it("parts: 2–8; rows: 1–12", () => {
    const ok = (parts: number, rows: number) =>
      StackedBarSpec.props.safeParse({
        title: "T",
        subtitle: "S",
        parts: Array.from({ length: parts }, (_, i) => `P${i}`),
        rows: Array.from({ length: rows }, () => ({ label: "R", values: ["dp_x"] })),
      }).success;
    expect(ok(2, 1)).toBe(true);
    expect(ok(8, 12)).toBe(true);
    expect(ok(1, 1)).toBe(false);
    expect(ok(9, 1)).toBe(false);
    expect(ok(2, 13)).toBe(false);
  });

  it("eight parts take cat-1 to cat-8 in order; the legend follows the parts", () => {
    const parts = Array.from({ length: 8 }, (_, i) => `P${i}`);
    const chart = stackedBarData(
      { title: "T", subtitle: "S", parts, rows: [{ label: "R", values: parts.map(() => "dp_a1") }] },
      data,
    );
    expect(chart.roles).toEqual(parts.map((_, i) => `cat-${i + 1}`));
    expect(stackLegend(chart).map((l) => [l.label, l.role, l.shape])).toEqual(
      parts.map((p, i) => [p, `cat-${i + 1}`, "square"]),
    );
  });

  it("a segmented bar: a highlighted part below zero is the neutral highlight and is not drawn; one above zero is crimson", () => {
    const seg = (highlight: string): SegmentedBarProps => ({
      title: "T",
      subtitle: "S",
      parts: [
        { label: "A", value: "dp_a2", highlight: highlight === "A" },
        { label: "B", value: "dp_b2", highlight: highlight === "B" },
        { label: "C", value: "dp_c2", highlight: highlight === "C" },
      ],
    });
    const fell = segmentedBarData(seg("B"), data);
    expect(fell.stack.roles).toEqual(["context", "highlight-neutral", "context"]);
    expect(fell.model?.rows[1]).toMatchObject({ highlight: true, fell: true });
    expect(stackPlot(fell.stack, "segmented", data).rows[0]?.segments).toHaveLength(2);
    const rose = segmentedBarData(seg("A"), data);
    expect(rose.stack.roles).toEqual(["highlight", "context", "context"]);
    expect(rose.model?.rows[0]).toMatchObject({ highlight: true, fell: false });
  });

  it("a segmented bar's stops are its drawn parts, one each, in block order", () => {
    const { stack } = segmentedBarData(
      {
        title: "T",
        subtitle: "S",
        parts: [
          { label: "A", value: "dp_a3" },
          { label: "B", value: "dp_b3" },
          { label: "C", value: "dp_c3" },
        ],
      },
      data,
    );
    const plot = stackPlot(stack, "segmented", data);
    expect(plot.stops.map((s) => s.title)).toEqual(["A", "C"]);
    expect(plot.stops.map((s) => s.tooltip[0]?.value)).toEqual(["40%", "60%"]);
    expect(
      SegmentedBarSpec.props.safeParse({ title: "T", subtitle: "S", parts: [{ label: "A", value: "dp_x" }] }).success,
    ).toBe(false);
  });
});

// ---------------------------------------------------------------------------------------------------------------

describe("DotPlot", () => {
  const data = resolverWith({
    dp_d1: { value: "10", unit: "%", currency: null, scale: null },
    dp_d2: { value: "12", unit: "%", currency: null, scale: null },
    dp_d3: { value: "15", unit: "%", currency: null, scale: null },
    dp_d4: { value: "9", unit: "%", currency: null, scale: null },
    dp_dn: { value: null, unit: "%", currency: null, scale: null },
    dp_dneg: { value: "-3", unit: "%", currency: null, scale: null },
  });
  const props = (rows: DotPlotProps["rows"], extra: Partial<DotPlotProps> = {}): DotPlotProps => ({
    title: "T",
    subtitle: "S",
    rows,
    ...extra,
  });

  it("sorts by the last value, largest first, with missing values last; 'given' keeps the order", () => {
    const rows = [
      { label: "Low", values: ["dp_d1"] },
      { label: "Gap", values: ["dp_dn"] },
      { label: "High", values: ["dp_d3"] },
      { label: "Mid", values: ["dp_d2"] },
    ];
    expect(dotPlotData(props(rows), data).rows.map((r) => r.input.label)).toEqual(["High", "Mid", "Low", "Gap"]);
    expect(dotPlotData(props(rows, { sort: "given" }), data).rows.map((r) => r.input.label)).toEqual([
      "Low",
      "Gap",
      "High",
      "Mid",
    ]);
  });

  it("dumbbell: the smaller value's label on its left, the larger's on its right, whichever comes first", () => {
    const chart = dotPlotData(
      props(
        [
          { label: "Rose", values: ["dp_d1", "dp_d3"] },
          { label: "Fell", values: ["dp_d3", "dp_d4"] },
          { label: "Tie", values: ["dp_d2", "dp_d2"] },
        ],
        { sort: "given", keys: ["Before", "After"] },
      ),
      data,
    );
    const plot = dotPlot(chart, data);
    expect(plot.rows.map((r) => r.dots.map((d) => d.side))).toEqual([
      ["left", "right"],
      ["right", "left"],
      ["left", "right"],
    ]);
    expect(dotLegend(chart).map((l) => [l.label, l.role, l.shape])).toEqual([
      ["Before", "context", "dot"],
      ["After", "comparison", "dot"],
    ]);
  });

  it("a missing end is 'No data' at the far end of the track from the other dot, and draws no line", () => {
    const plot = dotPlot(
      dotPlotData(
        props(
          [
            { label: "Gap", values: ["dp_d1", "dp_dn"] },
            { label: "Both", values: ["dp_d3", "dp_d4"] },
          ],
          { sort: "given" },
        ),
        data,
      ),
      data,
    );
    const gap = plot.rows[0];
    expect(gap?.line).toBeNull();
    expect(gap?.dots[1]).toMatchObject({ x: null, value: "No data", side: "right" });
    expect(gap?.dots[0]?.x).toBeLessThan(50);
    // D-1: the present dot's value goes on the side away from "No data", so the two can never overlap, and the
    // label room on each side holds "No data" too (it sits past the track's end).
    expect(gap?.dots[0]?.side).toBe("left");
    expect(Number.parseInt(plot.room.right, 10)).toBeGreaterThanOrEqual("No data".length + 1);
    // A dot plot (one value) with its only value missing: "No data" in the left room.
    const single = dotPlot(
      dotPlotData(
        props([
          { label: "Gap", values: ["dp_dn"] },
          { label: "P", values: ["dp_d1"] },
        ]),
        data,
      ),
      data,
    );
    const missing = single.rows.find((r) => r.label === "Gap")?.dots[0];
    expect(missing).toMatchObject({ x: null, side: "left" });
    expect(Number.parseInt(single.room.left, 10)).toBeGreaterThanOrEqual("No data".length + 1);
  });

  it("the fell rule: one value below zero, or a dumbbell's second below its first", () => {
    const single = dotPlot(
      dotPlotData(
        props([
          { label: "Neg", values: ["dp_dneg"], highlight: true },
          { label: "P", values: ["dp_d1"] },
        ]),
        data,
      ),
      data,
    );
    expect(single.rows.find((r) => r.label === "Neg")?.dots[0]?.role).toBe("highlight-neutral");
    expect(single.rows.find((r) => r.label === "P")?.dots[0]?.role).toBe("context");
    const dumb = (values: string[]) =>
      dotPlot(
        dotPlotData(
          props([
            { label: "H", values, highlight: true },
            { label: "O", values: ["dp_d1", "dp_d2"] },
          ]),
          data,
        ),
        data,
      )
        .rows.find((r) => r.label === "H")
        ?.dots.map((d) => d.role);
    expect(dumb(["dp_d3", "dp_d4"])).toEqual(["context", "highlight-neutral"]);
    expect(dumb(["dp_d1", "dp_d3"])).toEqual(["context", "highlight"]);
    expect(dumb(["dp_dn", "dp_d1"])).toEqual(["context", "highlight"]);
  });

  it("no highlight: one value per row is cat-1 throughout", () => {
    const plot = dotPlot(
      dotPlotData(
        props([
          { label: "A", values: ["dp_d1"] },
          { label: "B", values: ["dp_d2"] },
        ]),
        data,
      ),
      data,
    );
    expect(plot.rows.flatMap((r) => r.dots.map((d) => d.role))).toEqual(["cat-1", "cat-1"]);
    expect(dotLegend(dotPlotData(props([{ label: "A", values: ["dp_d1"] }]), data))).toEqual([]);
  });
});

// ---------------------------------------------------------------------------------------------------------------

describe("Slope", () => {
  const points: Record<string, Point> = {};
  for (let i = 0; i < 12; i++) {
    points[`dp_f${i}`] = { value: String(10 + (i % 3) * 0.1), unit: "%", currency: null, scale: null };
    points[`dp_t${i}`] = { value: String(11 + (i % 2) * 0.1), unit: "%", currency: null, scale: null };
  }
  points.dp_up = { value: "5", unit: "%", currency: null, scale: null };
  points.dp_down = { value: "3", unit: "%", currency: null, scale: null };
  const data = resolverWith(points);
  const many = (n: number, highlight = -1): SlopeProps => ({
    title: "T",
    subtitle: "S",
    periods: ["Before", "After"],
    items: Array.from({ length: n }, (_, i) => ({
      label: `Item ${i + 1} with a name long enough to wrap in the right-hand gutter`,
      from: `dp_f${i}`,
      to: `dp_t${i}`,
      highlight: i === highlight,
    })),
  });

  it("beyond eight items with nothing highlighted, the rest are neutral, never a generated colour", () => {
    const roles = slopeData(many(12), data).items.map((i) => i.role);
    expect(roles.slice(0, 8)).toEqual(Array.from({ length: 8 }, (_, i) => `cat-${i + 1}`));
    expect(roles.slice(8)).toEqual(["neutral", "neutral", "neutral", "neutral"]);
    expect(SlopeSpec.props.safeParse(many(13)).success).toBe(false);
  });

  it("twelve nearly equal items: labels on each side never overlap, and the plot grows to fit them", () => {
    const plot = slopePlot(slopeData(many(12, 4), data), data);
    expect(plot.height).toBeGreaterThan(288);
    const unit = (pct: number) => (pct / 100) * plot.height;
    const lines = (it: (typeof plot.items)[number]) => wrapLines(`${it.to.value} ${it.label}`, 18);
    for (const side of ["from", "to"] as const) {
      const sorted = [...plot.items].sort((a, b) => a[side].labelTop - b[side].labelTop);
      for (let i = 1; i < sorted.length; i++) {
        const prev = sorted[i - 1] as (typeof sorted)[number];
        const h = (side === "from" ? 1 : lines(prev)) * SANKEY.lineHeight;
        expect(
          unit((sorted[i] as (typeof sorted)[number])[side].labelTop),
          `${side} ${prev.label}`,
        ).toBeGreaterThanOrEqual(unit(prev[side].labelTop) + h - 1e-6);
      }
    }
    // The highlight is drawn last, on top.
    expect(plot.items.at(-1)?.highlight).toBe(true);
  });

  it("tight layout (C2-4): one-line right-hand values stack apart inside the plot, in value order, with room sized to the widest", () => {
    const plot = slopePlot(slopeData(many(12, 4), data), data);
    const unit = (pct: number) => (pct / 100) * plot.height;
    const sorted = [...plot.items].sort((a, b) => a.tightTop - b.tightTop);
    for (let i = 1; i < sorted.length; i++) {
      const prev = sorted[i - 1] as (typeof sorted)[number];
      expect(unit((sorted[i] as (typeof sorted)[number]).tightTop), prev.label).toBeGreaterThanOrEqual(
        unit(prev.tightTop) + SANKEY.lineHeight - 1e-6,
      );
    }
    expect(unit((sorted.at(-1) as (typeof sorted)[number]).tightTop) + SANKEY.lineHeight).toBeLessThanOrEqual(
      plot.height + 1e-6,
    );
    // Top to bottom is highest value to lowest, so the list below the plot reads in the same order.
    const ys = sorted.map((it) => it.to.y ?? 0);
    expect(ys).toEqual([...ys].sort((a, b) => a - b));
    expect(plot.endRoom).toBe(Math.max(...plot.items.map((it) => [...it.to.value].length)));
  });

  it("the fell rule: the highlighted item's second value below its first", () => {
    const two = (from: string, to: string): SlopeProps => ({
      title: "T",
      subtitle: "S",
      periods: ["A", "B"],
      items: [
        { label: "H", from, to, highlight: true },
        { label: "O", from: "dp_f0", to: "dp_t0" },
      ],
    });
    expect(slopeData(two("dp_up", "dp_down"), data).items[0]?.role).toBe("highlight-neutral");
    expect(slopeData(two("dp_down", "dp_up"), data).items[0]?.role).toBe("highlight");
    expect(slopeData(two("dp_up", "dp_up"), data).items[0]?.role).toBe("highlight");
    expect(slopeData(two("dp_up", "dp_down"), data).model?.rows[0]).toMatchObject({ highlight: true, fell: true });
  });
});
