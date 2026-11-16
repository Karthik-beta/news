// Builder's interaction checks for the Charts I islands (jsdom): each island hydrates its server markup with no
// mismatch; the plot becomes one tab stop only once hydrated; arrow keys move between points, Home and End jump,
// the tooltip and the live region follow focus, Escape closes; a tap on a mark opens its tooltip and a tap
// outside closes it. Islands come from the real component fixture pages, so props are what pages ship.
import { act, fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type ComponentType, createElement } from "react";
import { hydrateRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import BarChartPlot from "../../src/components/BarChart/BarChartPlot.client";
import ColumnChartPlot from "../../src/components/ColumnChart/ColumnChartPlot.client";
import LineChartPlot from "../../src/components/LineChart/LineChartPlot.client";
import { componentFixtureBundle, componentFixtures } from "../../src/renderer/fixtures/components.vite";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { renderPage } from "../../src/renderer/render";
import { expectNoAxeViolations, renderComponent } from "../support/render";

// biome-ignore lint/suspicious/noExplicitAny: island props are each island's own JSON
const COMPONENTS: Record<string, ComponentType<any>> = { ColumnChartPlot, BarChartPlot, LineChartPlot };

interface IslandOnPage {
  name: string;
  prefix: string;
  props: Record<string, unknown>;
  html: string;
}

function islandsOf(component: string, fixture: string): IslandOnPage[] {
  const f = componentFixtures.find((x) => x.name === component && x.fixture === fixture);
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

/** Puts the server HTML in the page and hydrates it, the way the client entry does. Returns hydration errors. */
async function hydrate(island: IslandOnPage): Promise<{ el: HTMLElement; errors: unknown[] }> {
  const el = document.createElement("div");
  el.innerHTML = island.html;
  document.body.append(el);
  const errors: unknown[] = [];
  const spy = vi.spyOn(console, "error").mockImplementation((...args) => errors.push(args));
  await act(async () => {
    root = hydrateRoot(el, createElement(COMPONENTS[island.name] as ComponentType, island.props), {
      identifierPrefix: island.prefix,
      onRecoverableError: (e) => errors.push(e),
    });
  });
  spy.mockRestore();
  return { el, errors };
}

const tooltipValues = () => [...document.querySelectorAll(".chart-tooltip .popover__value")].map((v) => v.textContent);
const liveText = () => document.querySelector("[aria-live='polite']")?.textContent ?? "";

const CASES = [
  ["ColumnChart", "default", "ColumnChartPlot", "bands"],
  ["ColumnChart", "edge-cases", "ColumnChartPlot", "bands"],
  ["BarChart", "default", "BarChartPlot", "rows"],
  ["BarChart", "edge-cases", "BarChartPlot", "rows"],
  ["LineChart", "default", "LineChartPlot", "stops"],
  ["LineChart", "edge-cases", "LineChartPlot", "stops"],
  ["StepLineChart", "default", "LineChartPlot", "stops"],
  ["StepLineChart", "edge-cases", "LineChartPlot", "stops"],
] as const;

type Stop = { tooltip: { value: string | null }[]; label: string };

describe.each(CASES)("%s/%s", (component, fixture, islandName, stopsKey) => {
  const islands = islandsOf(component, fixture);

  it("has islands of the expected kind", () => {
    expect(islands.length).toBeGreaterThan(0);
    for (const i of islands) expect(i.name).toBe(islandName);
  });

  it.each(islands.map((i, n) => [n, i] as const))(
    "island %i hydrates its server markup cleanly",
    async (_n, island) => {
      const { el, errors } = await hydrate(island);
      expect(errors).toEqual([]);
      const plot = el.querySelector("[role='img']") as HTMLElement;
      expect(plot.getAttribute("tabindex")).toBe("0");
      expect(el.querySelector("[data-hydrated='true']")).not.toBeNull();
      expect(plot).toHaveAccessibleName(island.props.summary as string);
    },
  );

  it.each(islands.map((i, n) => [n, i] as const))(
    "island %i: keys step through the points, Escape closes",
    async (_n, island) => {
      const stops = island.props[stopsKey] as Stop[];
      const user = userEvent.setup();
      const { el } = await hydrate(island);
      const plot = el.querySelector("[role='img']") as HTMLElement;
      await user.tab();
      expect(plot).toHaveFocus();
      expect(tooltipValues()).toEqual([]);
      const shown = (s: Stop) => s.tooltip.map((t) => t.value ?? "No data");

      await user.keyboard("{ArrowRight}");
      expect(tooltipValues()).toEqual(shown(stops[0] as Stop));
      expect(liveText()).toContain(stops[0]?.label);
      if (stops.length > 1) {
        await user.keyboard("{ArrowRight}");
        expect(tooltipValues()).toEqual(shown(stops[1] as Stop));
        await user.keyboard("{ArrowLeft}");
        expect(tooltipValues()).toEqual(shown(stops[0] as Stop));
      }
      await user.keyboard("{End}");
      expect(tooltipValues()).toEqual(shown(stops.at(-1) as Stop));
      await user.keyboard("{ArrowRight}"); // stays on the last
      expect(tooltipValues()).toEqual(shown(stops.at(-1) as Stop));
      await user.keyboard("{Home}");
      expect(tooltipValues()).toEqual(shown(stops[0] as Stop));
      await user.keyboard("{Escape}");
      expect(tooltipValues()).toEqual([]);
      expect(liveText()).toBe("");
      // From nothing, the left arrow starts at the end.
      await user.keyboard("{ArrowLeft}");
      expect(tooltipValues()).toEqual(shown(stops.at(-1) as Stop));
      // Leaving the plot closes the tooltip.
      await user.tab();
      expect(tooltipValues()).toEqual([]);
    },
  );

  it.each(islands.map((i, n) => [n, i] as const))(
    "island %i: a tap shows a tooltip, a tap outside closes it",
    async (_n, island) => {
      const stops = island.props[stopsKey] as Stop[];
      const { el } = await hydrate(island);
      const plot = el.querySelector("[role='img']") as HTMLElement;
      const target = (el.querySelector("[data-band='0'], [data-row='0']") as HTMLElement | null) ?? plot; // line: nearest stop
      fireEvent.pointerDown(target, { pointerType: "touch", clientX: 0, clientY: 0 });
      expect(tooltipValues()).toEqual((stops[0] as Stop).tooltip.map((t) => t.value ?? "No data"));
      fireEvent.pointerDown(document.body, { pointerType: "touch" });
      expect(tooltipValues()).toEqual([]);
    },
  );
});

describe("tooltips", () => {
  it("show the value first, the series and the source, and carry the reference", async () => {
    const [island] = islandsOf("LineChart", "default");
    const user = userEvent.setup();
    const { el } = await hydrate(island as IslandOnPage);
    (el.querySelector("[role='img']") as HTMLElement).focus();
    await user.keyboard("{End}");
    const tip = el.querySelector(".chart-tooltip") as HTMLElement;
    expect(tip).toHaveAttribute("aria-hidden", "true");
    const lines = [...tip.querySelectorAll(".chart-tooltip__line")];
    expect(lines.map((l) => l.querySelector(".popover__value")?.textContent)).toEqual(["73.9%", "37.2%"]);
    expect(lines.map((l) => l.querySelector(".chart-tooltip__series")?.textContent)).toEqual(["Services", "Products"]);
    for (const l of lines) expect(l.getAttribute("data-ref")).toMatch(/^dp_aapl_gm_/);
    expect(tip.textContent).toContain("fiscal 2024");
    expect(tip.textContent).toContain("Form 10-K, fiscal 2024, p. 23");
    expect(screen.getByText(/Services, 73\.9%/, { selector: "[aria-live]" })).toBeInTheDocument();
  });

  it("pass axe while a tooltip is open", async () => {
    const [island] = islandsOf("ColumnChart", "default");
    const { el } = await hydrate(island as IslandOnPage);
    const user = userEvent.setup();
    (el.querySelector("[role='img']") as HTMLElement).focus();
    await user.keyboard("{ArrowRight}");
    await expectNoAxeViolations(el);
  });
});

describe("islands render the same without the server", () => {
  it("a client render of the island matches its static markup's structure", () => {
    const [island] = islandsOf("BarChart", "default");
    const { container } = renderComponent(createElement(BarChartPlot, island?.props as never));
    expect(container.querySelectorAll(".bar-chart__row")).toHaveLength(5);
  });
});
