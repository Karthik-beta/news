// Component tester's island checks for Charts III part 1 (#13), in jsdom: every island on the SmallMultiples,
// Histogram and Heatmap fixture pages (LineChartPlot and ColumnChartPlot with shared scales or pinned ends, and
// HeatmapPlot) hydrates its server markup with no warnings; the plot is one named tab stop only once hydrated; the
// keys reach every stop in order, Escape and leaving close; a heatmap's up and down keys move a row and stop at the
// edges; a tap or a mouse on a heatmap cell opens its tooltip; every tooltip line with a value carries its reference.
import { act, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type ComponentType, createElement } from "react";
import { hydrateRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import ColumnChartPlot from "../../src/components/ColumnChart/ColumnChartPlot.client";
import HeatmapPlot from "../../src/components/Heatmap/HeatmapPlot.client";
import LineChartPlot from "../../src/components/LineChart/LineChartPlot.client";
import { componentFixtureBundle, componentFixtures } from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { renderPage } from "../../src/renderer/render";

// biome-ignore lint/suspicious/noExplicitAny: island props are each island's own JSON
const COMPONENTS: Record<string, ComponentType<any>> = { ColumnChartPlot, HeatmapPlot, LineChartPlot };

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

const stopsOf = (island: IslandOnPage): Stop[] => {
  const p = island.props;
  if (island.name === "HeatmapPlot")
    return (p.rows as { cells: { tooltip: TooltipLine }[] }[]).flatMap((r) =>
      r.cells.map((c) => ({ tooltip: [c.tooltip] })),
    );
  if (island.name === "LineChartPlot") return p.stops as Stop[];
  return p.bands as Stop[];
};

const tooltipValues = () => [...document.querySelectorAll(".chart-tooltip .popover__value")].map((v) => v.textContent);
const tooltipText = () => document.querySelector(".chart-tooltip")?.textContent ?? "";
const liveText = () => document.querySelector("[aria-live='polite']")?.textContent ?? "";
const shown = (s: Stop) => s.tooltip.map((t) => t.value ?? "No data");

const PAGES = ["SmallMultiples", "Histogram", "Heatmap"].flatMap((c) =>
  ["default", "edge-cases"].map((f) => [c, f] as const),
);

describe.each(PAGES)("%s/%s", (component, fixture) => {
  const islands = islandsOf(component, fixture);

  it("has islands, and only the plots it should", () => {
    expect(islands.length).toBeGreaterThan(0);
    const expected =
      component === "Heatmap"
        ? ["HeatmapPlot"]
        : component === "Histogram"
          ? ["ColumnChartPlot"]
          : ["LineChartPlot", "ColumnChartPlot"];
    for (const i of islands) expect(expected).toContain(i.name);
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
        await user.keyboard("{ArrowRight}");
        expect(tooltipValues(), `stop ${i + 1}`).toEqual(shown(stop));
        expect(liveText()).not.toBe("");
      }
      await user.keyboard("{ArrowRight}");
      expect(tooltipValues(), "stays on the last").toEqual(shown(stops.at(-1) as Stop));
      await user.keyboard("{Home}");
      expect(tooltipValues()).toEqual(shown(stops[0] as Stop));
      await user.keyboard("{End}");
      expect(tooltipValues()).toEqual(shown(stops.at(-1) as Stop));
      await user.keyboard("{Escape}");
      expect(tooltipValues()).toEqual([]);
      expect(liveText()).toBe("");
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

describe("Heatmap: two-dimensional keys and pointers", () => {
  const island = islandsOf("Heatmap", "edge-cases").find((i) => i.name === "HeatmapPlot") as IslandOnPage; // matrix
  const rows = island.props.rows as { label: string; cells: { tooltip: TooltipLine }[] }[];
  const columns = island.props.columns as number;
  const periods = island.props.periods as string[];
  const cell = (r: number, c: number) => `${rows[r]?.label}, ${periods[c]}`;

  it("down and up move a row in the same column and stop at the first and last rows; right wraps to the next row", async () => {
    expect(rows.length).toBeGreaterThan(2);
    const user = userEvent.setup();
    await hydrate(island);
    await user.tab();
    await user.keyboard("{ArrowDown}");
    expect(tooltipText(), "from nothing, down starts at the first cell").toContain(cell(0, 0));
    await user.keyboard("{ArrowRight}{ArrowRight}");
    await user.keyboard("{ArrowDown}");
    expect(tooltipText()).toContain(cell(1, 2));
    expect(liveText()).toContain(cell(1, 2));
    await user.keyboard("{ArrowUp}{ArrowUp}");
    expect(tooltipText(), "up stops at the first row").toContain(cell(0, 2));
    await user.keyboard("{End}");
    await user.keyboard("{ArrowDown}");
    expect(tooltipText(), "down stops at the last row").toContain(cell(rows.length - 1, columns - 1));
    await user.keyboard("{Home}");
    for (let i = 0; i < columns; i++) await user.keyboard("{ArrowRight}");
    expect(tooltipText(), "right from a row's last cell goes to the next row").toContain(cell(1, 0));
  });

  it("a hatched cell's tooltip says No data; a tap opens a cell and a tap outside closes it; a mouse follows", async () => {
    const { el } = await hydrate(island);
    const hatched = el.querySelector(".heatmap__cell.chart-hatch") as HTMLElement;
    expect(hatched).not.toBeNull();
    fireEvent.pointerDown(hatched, { pointerType: "touch" });
    expect(tooltipValues()).toEqual(["No data"]);
    fireEvent.pointerDown(document.body, { pointerType: "touch" });
    expect(tooltipValues()).toEqual([]);
    const cells = el.querySelectorAll<HTMLElement>(".heatmap__cell");
    fireEvent.pointerMove(cells[3] as HTMLElement, { pointerType: "mouse" });
    expect(tooltipText()).toContain(cell(0, 3));
    fireEvent.pointerLeave(el.querySelector(".heatmap__grid") as HTMLElement, { pointerType: "mouse" });
    expect(tooltipValues()).toEqual([]);
    // A touch move (a scroll) never opens a tooltip.
    fireEvent.pointerMove(cells[4] as HTMLElement, { pointerType: "touch" });
    expect(tooltipValues()).toEqual([]);
  });
});

describe("Histogram tooltips", () => {
  it("each range lists its values with their items, up to 8, and the plot writes no count", async () => {
    const island = islandsOf("Histogram", "default").at(-1) as IslandOnPage; // sixteen items
    const bands = island.props.bands as { label: string; tooltip: { value: string; series: string }[] }[];
    const user = userEvent.setup();
    const { el } = await hydrate(island);
    expect(el.querySelectorAll(".column-chart__value")).toHaveLength(0);
    await user.tab();
    for (const band of bands) {
      await user.keyboard("{ArrowRight}");
      expect(tooltipText()).toContain(band.label);
      expect(tooltipValues()).toEqual(band.tooltip.map((t) => t.value));
      expect(band.tooltip.length).toBeLessThanOrEqual(8);
    }
  });
});
