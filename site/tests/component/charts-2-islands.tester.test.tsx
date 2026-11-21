// Component tester's island checks for Charts II (#13, part 1), in jsdom: every island on every Charts II fixture
// page (SankeyPlot, StackedBarPlot for stacked and segmented bars, DotPlotPlot, SlopePlot, and the BarChartPlot a
// waterfall reuses) hydrates its server markup with no warnings; the plot is one tab stop only once hydrated and is
// named by its summary; the arrow keys step through every stop in order, Home and End jump, the tooltip and the live
// region follow, Escape closes, leaving closes; a tap on a mark opens its tooltip and a tap outside closes it; a
// pointer on a Sankey flow shows its node; every tooltip line carries its data reference.
import { act, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type ComponentType, createElement } from "react";
import { hydrateRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import BarChartPlot from "../../src/components/BarChart/BarChartPlot.client";
import DotPlotPlot from "../../src/components/DotPlot/DotPlotPlot.client";
import SankeyPlot from "../../src/components/Sankey/SankeyPlot.client";
import SlopePlot from "../../src/components/Slope/SlopePlot.client";
import StackedBarPlot from "../../src/components/StackedBar/StackedBarPlot.client";
import { componentFixtureBundle, componentFixtures } from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { renderPage } from "../../src/renderer/render";

// biome-ignore lint/suspicious/noExplicitAny: island props are each island's own JSON
const COMPONENTS: Record<string, ComponentType<any>> = {
  BarChartPlot,
  DotPlotPlot,
  SankeyPlot,
  SlopePlot,
  StackedBarPlot,
};

interface IslandOnPage {
  name: string;
  prefix: string;
  props: Record<string, unknown>;
  html: string;
}

function islandsOf(component: string, fixture: string): IslandOnPage[] {
  const f = componentFixtures.find((x) => x.component === component && x.fixture === fixture);
  if (!f) throw new Error(`no fixture ${component}/${fixture}`);
  const { article } = renderPage(componentFixtureBundle(f), { library: fixtureLibrary() });
  const doc = new DOMParser().parseFromString(article, "text/html");
  return [...doc.querySelectorAll<HTMLElement>("[data-island]")].map((el) => ({
    name: el.dataset.island as string,
    prefix: el.dataset.islandPrefix as string,
    props: JSON.parse(el.dataset.props as string),
    html: el.innerHTML,
  }));
}

let root: Root | null = null;
afterEach(() => {
  act(() => root?.unmount());
  root = null;
  document.body.innerHTML = "";
});

async function hydrate(island: IslandOnPage): Promise<{ el: HTMLElement; errors: unknown[] }> {
  const el = document.createElement("div");
  el.innerHTML = island.html;
  document.body.append(el);
  const errors: unknown[] = [];
  const error = vi.spyOn(console, "error").mockImplementation((...args) => errors.push(args));
  const warn = vi.spyOn(console, "warn").mockImplementation((...args) => errors.push(args));
  await act(async () => {
    root = hydrateRoot(el, createElement(COMPONENTS[island.name] as ComponentType, island.props), {
      identifierPrefix: island.prefix,
      onRecoverableError: (e) => errors.push(e),
    });
  });
  error.mockRestore();
  warn.mockRestore();
  return { el, errors };
}

interface TooltipLine {
  value: string | null;
  ref: string | null;
}
interface Stop {
  tooltip: TooltipLine[];
}

/** The stops an island steps through, in order (each island names them differently). */
const stopsOf = (island: IslandOnPage): Stop[] => {
  const p = island.props;
  if (island.name === "SankeyPlot") return p.nodes as Stop[];
  if (island.name === "SlopePlot") return p.items as Stop[];
  if (island.name === "StackedBarPlot") return p.stops as Stop[];
  return p.rows as Stop[];
};

const tooltipValues = () => [...document.querySelectorAll(".chart-tooltip .popover__value")].map((v) => v.textContent);
const liveText = () => document.querySelector("[aria-live='polite']")?.textContent ?? "";
const shown = (s: Stop) => s.tooltip.map((t) => t.value ?? "No data");

const PAGES = ["Waterfall", "Sankey", "StackedBar", "SegmentedBar", "DotPlot", "Slope"].flatMap((c) =>
  ["default", "edge-cases"].map((f) => [c, f] as const),
);

describe.each(PAGES)("%s/%s", (component, fixture) => {
  const islands = islandsOf(component, fixture).filter((i) => stopsOf(i).length > 0);

  it("has islands", () => {
    expect(islands.length).toBeGreaterThan(0);
  });

  it.each(islands.map((i, n) => [`${n} (${i.name})`, i] as const))(
    "island %s hydrates its server markup with no warnings, and only then is a named tab stop",
    async (_n, island) => {
      const before = document.createElement("div");
      before.innerHTML = island.html;
      expect(before.querySelector("[role='img']")?.getAttribute("tabindex"), "no tab stop before hydration").toBeNull();
      const { el, errors } = await hydrate(island);
      expect(errors).toEqual([]);
      const plot = el.querySelector("[role='img']") as HTMLElement;
      expect(plot.getAttribute("tabindex")).toBe("0");
      expect(plot).toHaveAccessibleName(island.props.summary as string);
      expect(el.querySelectorAll("[tabindex='0'], button, a[href]")).toHaveLength(1);
    },
  );

  it.each(islands.map((i, n) => [`${n} (${i.name})`, i] as const))(
    "island %s: the keys reach every stop in order, Escape and leaving close",
    async (_n, island) => {
      const stops = stopsOf(island);
      const user = userEvent.setup();
      const { el } = await hydrate(island);
      const plot = el.querySelector("[role='img']") as HTMLElement;
      await user.tab();
      expect(plot).toHaveFocus();
      for (const [i, stop] of stops.entries()) {
        await user.keyboard("{ArrowDown}");
        expect(tooltipValues(), `stop ${i + 1}`).toEqual(shown(stop));
        expect(liveText()).not.toBe("");
      }
      await user.keyboard("{ArrowRight}");
      expect(tooltipValues(), "stays on the last").toEqual(shown(stops.at(-1) as Stop));
      await user.keyboard("{Home}");
      expect(tooltipValues()).toEqual(shown(stops[0] as Stop));
      await user.keyboard("{End}");
      expect(tooltipValues()).toEqual(shown(stops.at(-1) as Stop));
      await user.keyboard("{ArrowUp}");
      if (stops.length > 1) expect(tooltipValues()).toEqual(shown(stops.at(-2) as Stop));
      await user.keyboard("{Escape}");
      expect(tooltipValues()).toEqual([]);
      expect(liveText()).toBe("");
      await user.keyboard("{ArrowLeft}");
      expect(tooltipValues(), "from nothing, left starts at the end").toEqual(shown(stops.at(-1) as Stop));
      await user.tab();
      expect(tooltipValues()).toEqual([]);
    },
  );

  it.each(islands.map((i, n) => [`${n} (${i.name})`, i] as const))(
    "island %s: every tooltip line with a value carries its data reference",
    (_n, island) => {
      for (const stop of stopsOf(island)) {
        for (const line of stop.tooltip) if (line.value !== null) expect(line.ref).toMatch(/^(dp|dv)_/);
      }
    },
  );
});

describe("pointers and taps", () => {
  it("a tap on each kind of mark opens that stop's tooltip; a tap outside closes it", async () => {
    for (const [component, mark] of [
      ["Sankey", ".sankey__node[data-node='3']"],
      ["StackedBar", ".stacked-bar__row:last-child .stacked-bar__segment"],
      ["SegmentedBar", ".stacked-bar__segment"],
      ["DotPlot", "[data-row='1'] .dot-plot__dot"],
      ["Slope", "[data-item='0'] .slope__dot"],
    ] as const) {
      const island = islandsOf(component, "default").find((i) => i.name !== "BarChartPlot") as IslandOnPage;
      const { el } = await hydrate(island);
      const target = el.querySelector(mark) as HTMLElement;
      expect(target, `${component}: ${mark}`).not.toBeNull();
      fireEvent.pointerDown(target, { pointerType: "touch", clientX: 0, clientY: 0 });
      expect(tooltipValues().length, component).toBeGreaterThan(0);
      fireEvent.pointerDown(document.body, { pointerType: "touch" });
      expect(tooltipValues(), component).toEqual([]);
      act(() => root?.unmount());
      root = null;
      document.body.innerHTML = "";
    }
  });

  it("a mouse on a Sankey flow shows the tooltip of the node it belongs to, and leaving hides it", async () => {
    const island = islandsOf("Sankey", "default").find((i) => i.name === "SankeyPlot") as IslandOnPage;
    const nodes = island.props.nodes as { label: string }[];
    const flows = island.props.flows as { owner: number }[];
    const { el } = await hydrate(island);
    const area = el.querySelector(".sankey__area") as HTMLElement;
    for (const [i, flow] of flows.entries()) {
      const path = el.querySelectorAll(".sankey__flow")[i] as SVGPathElement;
      fireEvent.pointerMove(path, { pointerType: "mouse" });
      expect(document.querySelector(".chart-tooltip")?.textContent, `flow ${i}`).toContain(nodes[flow.owner]?.label);
    }
    fireEvent.pointerLeave(area, { pointerType: "mouse" });
    expect(tooltipValues()).toEqual([]);
  });

  it("the Sankey's 'Other' tooltip lists each folded line with its value", async () => {
    const island = islandsOf("Sankey", "edge-cases").find((i) => i.name === "SankeyPlot") as IslandOnPage;
    const nodes = island.props.nodes as { label: string; tooltip: TooltipLine[] }[];
    const index = nodes.findIndex((n) => n.label === "Other");
    const { el } = await hydrate(island);
    fireEvent.pointerDown(el.querySelector(`.sankey__node[data-node='${index}']`) as HTMLElement, {
      pointerType: "touch",
    });
    const series = [...document.querySelectorAll(".chart-tooltip .chart-tooltip__series")].map((s) => s.textContent);
    expect(series).toEqual(["Segment E", "Segment F", "Segment G"]);
    expect(tooltipValues().every((v) => v !== "No data")).toBe(true);
  });
});
