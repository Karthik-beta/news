// Builder's checks for Charts II (#13, part 1): Waterfall, Sankey, StackedBar, SegmentedBar, DotPlot and Slope on
// their fixture pages. Flow and value sums in the Sankey and waterfall fixtures equal the source rows, exactly; no
// flow is drawn thinner than 2px; small flows fold into "Other"; a Sankey that cannot flow shows its waterfall; a
// subject that fell is never crimson. The shared checks (every number cited, CSV equals the table, Markdown) run
// for these charts in data-charts.test.ts. The formal tests and sign-off are the component tester's.
import type { ElementNode } from "@openuidev/react-lang";
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import {
  foldSmall,
  SANKEY,
  sankeyGraph,
  sankeyLayout,
  stackLabels,
  wrapLines,
} from "../../src/components/Sankey/layout";
import { type SankeyProps, sankeyData } from "../../src/components/Sankey/Sankey";
import {
  addExact,
  compareExact,
  negateExact,
  sumExact,
  waterfallMismatches,
  waterfallSteps,
} from "../../src/components/Waterfall/steps";
import { type WaterfallProps, waterfallData } from "../../src/components/Waterfall/Waterfall";
import { PageData, PageSources } from "../../src/renderer/bundle/schema";
import { PageDataResolver } from "../../src/renderer/data/resolve";
import { componentFixtureBundle, componentFixtures } from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { parsePage, renderPage } from "../../src/renderer/render";

const library = fixtureLibrary();
const NAMES = ["Waterfall", "Sankey", "StackedBar", "SegmentedBar", "DotPlot", "Slope"];

const pages = componentFixtures
  .filter((f) => NAMES.includes(f.component))
  .map((f) => {
    const input = componentFixtureBundle(f);
    const result = renderPage(input, { library });
    const doc = new JSDOM(result.article).window.document;
    const resolver = () => new PageDataResolver(PageData.parse(input.data), PageSources.parse(input.sources));
    return { id: `${f.component}/${f.fixture}`, f, result, doc, resolver };
  });

const page = (id: string) => {
  const p = pages.find((x) => x.id === id);
  if (!p) throw new Error(`no fixture page ${id}`);
  return p;
};
const figure = (doc: Document, statement: string) => {
  const el = doc.getElementById(`${statement}-title`)?.closest("figure");
  if (!el) throw new Error(`no figure for ${statement}`);
  return el;
};
const islandProps = (el: Element, name: string) =>
  JSON.parse(el.querySelector(`[data-island="${name}"]`)?.getAttribute("data-props") ?? "null");

/** The props of every statement of one component in a fixture, as the page writer gave them. */
function statements<P>(id: string, type: string): { name: string; props: P }[] {
  const root = parsePage(page(id).f.oui, library.library);
  const out: { name: string; props: P }[] = [];
  const visit = (v: unknown) => {
    if (Array.isArray(v)) return v.forEach(visit);
    if (!v || typeof v !== "object" || (v as ElementNode).type !== "element") return;
    const node = v as ElementNode;
    if (node.typeName === type) out.push({ name: node.statementId ?? "", props: node.props as P });
    Object.values(node.props ?? {}).forEach(visit);
  };
  visit(root);
  return out;
}

describe("fixtures", () => {
  it("every Charts II component has a default and an edge-case fixture, and the defaults resolve cleanly", () => {
    for (const name of NAMES) {
      expect(page(`${name}/default`).result.problems, name).toEqual([]);
      expect(page(`${name}/edge-cases`), name).toBeTruthy();
    }
  });
});

describe("exact arithmetic", () => {
  it("adds, negates, sums and compares decimals exactly", () => {
    expect(addExact("0.1", "0.2")).toBe("0.3");
    expect(addExact("391035", "-210352")).toBe("180683");
    expect(addExact("-150", "20")).toBe("-130");
    expect(negateExact("-20")).toBe("20");
    expect(sumExact(["201183", "29984", "26694", "37005", "96169"])).toBe("391035");
    expect(sumExact([])).toBe("0");
    expect(compareExact("1.50", "1.5")).toBe(0);
    expect(compareExact("-0.8", "0")).toBe(-1);
  });

  it("a deduction published as a credit (negative) goes up; an addition published negative goes down", () => {
    const steps = waterfallSteps([
      { kind: "total", value: "100" },
      { kind: "deduct", value: "-20" },
      { kind: "add", value: "-5" },
      { kind: "total", value: "115" },
    ]);
    expect(steps.map((s) => [s.start, s.end])).toEqual([
      ["0", "100"],
      ["100", "120"],
      ["120", "115"],
      ["0", "115"],
    ]);
  });

  it("after a missing value the running amount is unknown until the next total", () => {
    const inputs = [
      { kind: "total" as const, value: "100" },
      { kind: "deduct" as const, value: null },
      { kind: "deduct" as const, value: "10" },
      { kind: "total" as const, value: "50" },
    ];
    expect(waterfallSteps(inputs).map((s) => s.start)).toEqual(["0", null, null, "0"]);
    expect(waterfallMismatches(inputs)).toEqual([]);
  });
});

describe("waterfall value sums equal the source rows", () => {
  it.each([
    ["Waterfall/default", "Waterfall"],
    ["Sankey/default", "Sankey"],
    ["Sankey/edge-cases", "Sankey"],
  ])("%s: every total is the lines above it, exactly, from data.json", (id, type) => {
    for (const { name, props } of statements<WaterfallProps>(id, type)) {
      const data = page(id).resolver();
      const chart = waterfallData({ ...props, rows: props.rows }, data);
      if (name === "mismatch" || name === "unknown") continue;
      expect(chart.mismatches, `${id} ${name}`).toEqual([]);
      // Recompute from the source rows themselves: start at the first total, apply each line as published.
      let running: string | null = null;
      for (const row of props.rows) {
        const v = data.value(row.value);
        if (!v.ok) {
          running = null;
          continue;
        }
        if (row.kind === "total") {
          if (running !== null) expect(compareExact(running, v.value), `${name}: ${row.label}`).toBe(0);
          running = v.value;
        } else if (running !== null) {
          running = addExact(running, row.kind === "deduct" ? negateExact(v.value) : v.value);
        }
      }
    }
  });

  it("a waterfall whose lines do not add up says so under the chart, in words", () => {
    const fig = figure(page("Waterfall/edge-cases").doc, "mismatch");
    expect(fig.querySelector(".waterfall__note")?.textContent).toBe(
      "The lines shown do not add up to Operating loss: the source has lines not shown here.",
    );
    expect(figure(page("Waterfall/default").doc, "profit").querySelector(".waterfall__note")).toBeNull();
  });

  it("deductions float between the running amounts and carry a minus; totals run from zero", () => {
    const props = islandProps(figure(page("Waterfall/default").doc, "profit"), "BarChartPlot");
    const byLabel = new Map(props.rows.map((r: { label: string }) => [r.label, r]));
    const sales = byLabel.get("Total net sales") as { start: number; width: number; role: string; value: string };
    const cost = byLabel.get("Total cost of sales") as { start: number; width: number; role: string; value: string };
    const margin = byLabel.get("Gross margin") as { start: number; width: number; role: string };
    const net = byLabel.get("Net income") as { role: string; value: string };
    expect([sales.start, sales.width, sales.role]).toEqual([0, 100, "comparison"]);
    expect(cost.role).toBe("neutral");
    expect(cost.value).toBe("−$210.4bn");
    expect(cost.start + cost.width).toBeCloseTo(100);
    expect(cost.start).toBeCloseTo(margin.width);
    expect(byLabel.get("Other income/(expense), net")).toMatchObject({ value: "+$269m", role: "neutral" });
    expect(net).toMatchObject({ role: "highlight", value: "$93.7bn" });
  });
});

describe("Sankey flows", () => {
  const sankeys = ["Sankey/default", "Sankey/edge-cases"].flatMap((id) =>
    statements<SankeyProps>(id, "Sankey").map((s) => ({ id, ...s })),
  );
  const chartOf = (s: (typeof sankeys)[number]) => sankeyData(s.props, page(s.id).resolver());

  it.each(sankeys.filter((s) => ["flows", "fold", "nosegments"].includes(s.name)).map((s) => [s.name, s] as const))(
    "%s: flows in and out of every node equal its source row, exactly",
    (_name, s) => {
      const chart = chartOf(s);
      const lines = [...chart.segments, ...chart.rows].map((l) => ({
        label: l.label,
        ref: l.ref,
        kind: l.kind,
        value: l.value?.value ?? null,
        shown: "",
      }));
      const graph = sankeyGraph(lines);
      expect(graph.ok).toBe(true);
      if (!graph.ok) return;
      const source = new Map(lines.map((l) => [l.ref, l.value as string]));
      graph.nodes.forEach((node, i) => {
        const own = source.get(node.items[0]?.ref as string) as string;
        const ins = graph.links.filter((l) => l.target === i).map((l) => l.value);
        const outs = graph.links.filter((l) => l.source === i).map((l) => l.value);
        if (ins.length > 0) expect(compareExact(sumExact(ins), own), `${node.label} in`).toBe(0);
        if (outs.length > 0) expect(compareExact(sumExact(outs), own), `${node.label} out`).toBe(0);
      });
      // Segments add up to the first total exactly.
      if (chart.segments.length > 0) {
        expect(sumExact(chart.segments.map((x) => x.value?.value as string))).toBe(chart.rows[0]?.value?.value);
      }
      // Folding keeps every amount: an "Other" flow is the sum of the lines folded into it.
      const folded = foldSmall(graph.nodes, graph.links, graph.revenue);
      folded.nodes.forEach((node, i) => {
        if (!node.folded) return;
        const flow = folded.links.find((l) => l.source === i || l.target === i);
        expect(flow?.value).toBe(sumExact(node.items.map((it) => source.get(it.ref) as string)));
      });
    },
  );

  it.each(sankeys.filter((s) => ["flows", "fold", "nosegments"].includes(s.name)).map((s) => [s.name, s] as const))(
    "%s: no flow is drawn thinner than 2px, and node bands hold their flows",
    (_name, s) => {
      const chart = chartOf(s);
      expect(chart.layout?.ok).toBe(true);
      if (!chart.layout?.ok) return;
      const { links, nodes, height } = chart.layout.layout;
      for (const l of links) expect(l.thickness, `${l.key}`).toBeGreaterThanOrEqual(SANKEY.minThickness);
      for (const [i, n] of nodes.entries()) {
        const ins = links.filter((l) => l.target === i).reduce((a, l) => a + l.thickness, 0);
        const outs = links.filter((l) => l.source === i).reduce((a, l) => a + l.thickness, 0);
        expect(n.height + 1e-6).toBeGreaterThanOrEqual(Math.max(ins, outs));
        expect(n.y + n.height).toBeLessThanOrEqual(height + 1e-6);
      }
      // 1 unit is 1px at the default text size: the plot's height is written in rem from the same units.
      const island = figure(page(s.id).doc, s.name).querySelector("[data-island='SankeyPlot'] .sankey__plot");
      expect(island?.getAttribute("style")).toContain(`height:${height / 16}rem`);
    },
  );

  it("flows under 1% of revenue fold into Other when two or more are that small; one small flow keeps its name", () => {
    const fold = sankeys.find((s) => s.name === "fold") as (typeof sankeys)[number];
    const layout = chartOf(fold).layout;
    if (!layout?.ok) throw new Error("fold must be drawable");
    const others = layout.layout.nodes.filter((n) => n.folded);
    expect(others.map((n) => n.items.map((i) => i.label))).toEqual([
      ["Segment E", "Segment F", "Segment G"],
      ["Impairment", "Litigation"],
    ]);
    expect(others.every((n) => n.label === "Other" && n.role === "neutral")).toBe(true);
    // Segment D is 1.5% of revenue: it keeps its own node.
    expect(layout.layout.nodes.some((n) => n.label === "Segment D")).toBe(true);
    const apple = chartOf(sankeys.find((s) => s.name === "flows") as (typeof sankeys)[number]).layout;
    if (!apple?.ok) throw new Error("Apple must be drawable");
    expect(apple.layout.nodes.some((n) => n.label === "Other income/(expense), net" && !n.folded)).toBe(true);
    // "Other" writes no number on the plot; its tooltip lists each folded line with its own reference.
    const props = islandProps(figure(page("Sankey/edge-cases").doc, "fold"), "SankeyPlot");
    const other = props.nodes.find((n: { label: string }) => n.label === "Other");
    expect(other.value).toBeNull();
    expect(other.tooltip.map((t: { ref: string }) => t.ref)).toEqual([
      "dp_fx_sk_seg_e",
      "dp_fx_sk_seg_f",
      "dp_fx_sk_seg_g",
    ]);
  });

  it("revenue flows are context, costs neutral, the profit path the highlight", () => {
    const props = islandProps(figure(page("Sankey/default").doc, "flows"), "SankeyPlot");
    expect(new Set(props.flows.map((f: { role: string }) => f.role))).toEqual(
      new Set(["context", "neutral", "highlight"]),
    );
    const net = props.nodes.find((n: { label: string }) => n.label === "Net income");
    expect(net).toMatchObject({ role: "highlight", result: true, value: "$93.7bn" });
  });

  it("the Sankey and its waterfall fallback are both in the page; a Sankey that cannot flow has only the waterfall", () => {
    const flows = figure(page("Sankey/default").doc, "flows");
    expect(flows.querySelector(".sankey.sankey--c5 .sankey__flows [data-island='SankeyPlot']")).not.toBeNull();
    expect(flows.querySelector(".sankey__fallback [data-island='BarChartPlot']")).not.toBeNull();
    for (const name of ["loss", "mismatch"]) {
      const fig = figure(page("Sankey/edge-cases").doc, name);
      expect(fig.querySelector(".sankey--fallback-only"), name).not.toBeNull();
      expect(fig.querySelector("[data-island='SankeyPlot']"), name).toBeNull();
      expect(fig.querySelector("[data-island='BarChartPlot']"), name).not.toBeNull();
    }
    const unknown = figure(page("Sankey/edge-cases").doc, "unknown");
    expect(unknown.querySelector(".chart-figure__missing")?.textContent).toBe("No data");
    expect(unknown.querySelector("[data-island]")).toBeNull();
  });

  it("says why rows cannot flow", () => {
    const row = (kind: "total" | "deduct" | "add" | "segment", value: string | null) => ({
      label: kind,
      ref: `dp_${kind}`,
      kind,
      value,
      shown: "",
    });
    expect(sankeyLayout([row("total", "10"), row("deduct", null)])).toEqual({ ok: false, reason: "missing" });
    expect(sankeyLayout([row("deduct", "1"), row("total", "10")])).toEqual({ ok: false, reason: "first-not-total" });
    expect(sankeyLayout([row("total", "10"), row("deduct", "20"), row("total", "-10")])).toEqual({
      ok: false,
      reason: "not-positive",
    });
    expect(sankeyLayout([row("total", "10"), row("deduct", "2"), row("total", "9")])).toEqual({
      ok: false,
      reason: "mismatch",
    });
    expect(sankeyLayout([row("segment", "4"), row("total", "10")])).toEqual({ ok: false, reason: "mismatch" });
    expect(sankeyLayout([row("total", "10"), row("add", "2")])).toEqual({ ok: false, reason: "trailing-addition" });
  });

  it("labels never overlap in a column, and the plot grows when they need more room", () => {
    const three = [
      { centre: 10, h: 40 },
      { centre: 12, h: 40 },
      { centre: 14, h: 40 },
    ];
    expect(stackLabels(three, 100).need).toBe(120);
    const { tops, need } = stackLabels(three, 120);
    expect(need).toBe(120);
    expect(tops).toEqual([0, 40, 80]);
    expect(wrapLines("Selling, general and administrative", 12)).toBe(4);
    expect(wrapLines("Other income/(expense), net", 12)).toBe(4);
    const fold = sankeys.find((s) => s.name === "flows") as (typeof sankeys)[number];
    const layout = chartOf(fold).layout;
    if (!layout?.ok) throw new Error("must be drawable");
    for (let c = 0; c < layout.layout.columns; c++) {
      const col = layout.layout.nodes.filter((n) => n.column === c).sort((a, b) => a.labelTop - b.labelTop);
      for (let j = 1; j < col.length; j++) {
        const prev = col[j - 1] as (typeof col)[number];
        const h = (wrapLines(prev.label) + (prev.folded ? 0 : 1)) * SANKEY.lineHeight + SANKEY.labelGap;
        expect((col[j] as (typeof col)[number]).labelTop + 1e-6).toBeGreaterThanOrEqual(prev.labelTop + h);
      }
    }
  });
});

describe("a subject that fell is never crimson", () => {
  const roleOf = (id: string, statement: string, island: string, pick: (props: never) => unknown) =>
    pick(islandProps(figure(page(id).doc, statement), island) as never);

  it("a waterfall's loss is the neutral highlight", () => {
    const rows = roleOf(
      "Waterfall/edge-cases",
      "loss",
      "BarChartPlot",
      (p: { rows: { label: string; role: string }[] }) => p.rows,
    ) as { label: string; role: string; negative: boolean; value: string }[];
    const net = rows.find((r) => r.label === "Net loss");
    expect(net).toMatchObject({ role: "highlight-neutral", negative: true, value: "−$130m" });
    expect(rows.find((r) => r.label === "Income tax")).toMatchObject({ value: "+$20m" });
    expect(figure(page("Waterfall/edge-cases").doc, "loss").querySelector(".chart-colour--highlight")).toBeNull();
  });

  it("dumbbell, slope, stacked and segmented subjects that fell take the neutral highlight", () => {
    const dumbbell = figure(page("DotPlot/edge-cases").doc, "fell");
    expect(dumbbell.querySelectorAll(".dot-plot__dot.chart-colour--highlight-neutral")).toHaveLength(1);
    expect(dumbbell.querySelector(".chart-colour--highlight")).toBeNull();
    const slope = figure(page("Slope/edge-cases").doc, "fell");
    expect(slope.querySelector(".slope__line.chart-colour--highlight-neutral")).not.toBeNull();
    expect(slope.querySelector(".chart-colour--highlight")).toBeNull();
    const stacked = figure(page("StackedBar/edge-cases").doc, "fell");
    expect(stacked.querySelector(".chart-colour--highlight")).toBeNull();
    expect(stacked.querySelectorAll(".stacked-bar__segment.chart-colour--highlight-neutral")).toHaveLength(2);
    const segmented = figure(page("SegmentedBar/edge-cases").doc, "fell");
    expect(segmented.querySelector(".chart-colour--highlight")).toBeNull();
    expect(segmented.querySelector(".segmented-bar__value.is-highlight-neutral")?.textContent).toBe("−0.8%");
  });

  it("a subject that rose is crimson", () => {
    expect(figure(page("DotPlot/default").doc, "dumbbell").querySelectorAll(".chart-colour--highlight")).toHaveLength(
      1,
    );
    expect(
      figure(page("Slope/default").doc, "europe").querySelector(".slope__line.chart-colour--highlight"),
    ).not.toBeNull();
    expect(
      figure(page("StackedBar/default").doc, "europe").querySelectorAll(
        ".stacked-bar__segment.chart-colour--highlight",
      ),
    ).toHaveLength(2);
  });
});

describe("missing data and other units", () => {
  it("a missing part or end is 'No data', never a zero; another unit is left out, never converted", () => {
    const seg = figure(page("SegmentedBar/edge-cases").doc, "missing");
    const values = [...seg.querySelectorAll(".segmented-bar__value")].map((v) => v.textContent);
    expect(values).toEqual(["8.0%", "No data", "0%", "6.1%", "No data"]);
    const slope = figure(page("Slope/edge-cases").doc, "gaps");
    expect([...slope.querySelectorAll(".slope__value.is-missing")].map((v) => v.textContent)).toEqual(["No data"]);
    expect([...slope.querySelectorAll(".slope__label--start.is-missing")].map((v) => v.textContent)).toEqual([
      "No data",
    ]);
    const other = figure(page("Waterfall/edge-cases").doc, "otherunit");
    expect(other.textContent).not.toContain("Different unit");
    const stacked = figure(page("StackedBar/edge-cases").doc, "years");
    expect(stacked.querySelectorAll(".stacked-bar__segment--missing")).toHaveLength(1);
  });

  it("unknown references show the frame with 'No data' and no island", () => {
    for (const id of NAMES.map((n) => `${n}/edge-cases`)) {
      const fig = figure(page(id).doc, "unknown");
      expect(fig.querySelector(".chart-figure__missing")?.textContent, id).toBe("No data");
      expect(fig.querySelector("[data-island]"), id).toBeNull();
    }
  });
});
