// Builder's sanity checks for the primitives (jsdom). The formal tests and sign-off are the component tester's
// (news-component-testing skill); these make sure every fixture case renders, passes axe, and that the contracts
// other components lean on hold: the popover is wired natively, missing data says so, decorations are hidden.
import { screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";
import type { FixtureCase } from "../../src/components/contract";
import { FigureFrame } from "../../src/components/FigureFrame/FigureFrame";
import { Icon } from "../../src/components/Icon/Icon";
import { Rule } from "../../src/components/Rule/Rule";
import { RefText } from "../../src/components/SourceRef/RefText";
import { SourceRef } from "../../src/components/SourceRef/SourceRef";
import { Text } from "../../src/components/Text/Text";
import { VisuallyHidden } from "../../src/components/VisuallyHidden/VisuallyHidden";
import { PageData, PageSources } from "../../src/renderer/bundle/schema";
import { PageDataProvider } from "../../src/renderer/data/context";
import { PageDataResolver } from "../../src/renderer/data/resolve";
import { mergeBundleData } from "../../src/renderer/fixtures/bundles/assemble";
import { fixtureBundles } from "../../src/renderer/fixtures/bundles/vite";
import { expectNoAxeViolations, renderComponent } from "../support/render";

const merged = mergeBundleData(fixtureBundles["apple-fy2024"], fixtureBundles["edge-cases"]);
const resolver = () => new PageDataResolver(PageData.parse(merged.data), PageSources.parse(merged.sources));
const withData = (ui: ReactNode, r = resolver()) => (
  <PageDataProvider resolver={r}>
    <main>{ui}</main>
  </PageDataProvider>
);

const modules = import.meta.glob<FixtureCase[]>("../../src/components/*/fixtures/cases.tsx", {
  import: "cases",
  eager: true,
});
const cases = Object.entries(modules).flatMap(([path, list]) =>
  list.map((c) => [`${/components\/([^/]+)\//.exec(path)?.[1]}: ${c.name}`, c] as const),
);

describe("primitive fixture cases", () => {
  it("cover every primitive", () => {
    const names = new Set(cases.map(([n]) => n.split(":")[0]));
    for (const p of ["FigureFrame", "Icon", "Popover", "Rule", "SourceRef", "Text", "VisuallyHidden"]) {
      expect(names.has(p), p).toBe(true);
    }
  });

  it.each(cases)("%s renders with no axe violations (light and dark)", async (_name, c) => {
    for (const theme of ["light", "dark"] as const) {
      const { container, unmount } = renderComponent(withData(c.render()), { theme });
      // The fixture sources list is not rendered here, so in-page source links have no target; that is the
      // gallery page's job (checked in the browser).
      await expectNoAxeViolations(container);
      unmount();
    }
  });
});

describe("SourceRef", () => {
  it("opens a native popover from a button: no JavaScript of its own", () => {
    renderComponent(withData(<SourceRef dataRef="dp_aapl_net_sales_fy2024" />));
    const button = screen.getByRole("button", { name: "$391.0bn" });
    const panel = document.getElementById(button.getAttribute("popovertarget") as string);
    expect(panel).toHaveAttribute("popover", "auto");
    expect(panel).toHaveAttribute("aria-label", "Source for $391.0bn");
    expect(panel).toHaveAttribute("role", "dialog");
    expect(panel?.textContent).toContain("$391,035 million");
    expect(panel?.textContent).toContain("XBRL tag us-gaap:RevenueFromContractWithCustomerExcludingAssessedTax");
  });

  it.each([
    ["dp_edge_null", "no-value"],
    ["dp_unknown_ref", "unknown-ref"],
  ])("renders %s as 'No data' with no button, and reports %s", (ref, code) => {
    const r = resolver();
    renderComponent(withData(<SourceRef dataRef={ref} />, r));
    expect(screen.getByText("No data")).toHaveAttribute("data-ref", ref);
    expect(screen.queryByRole("button")).toBeNull();
    expect(r.problems.map((p) => p.code)).toContain(code);
  });

  it("records the reference and its source as used", () => {
    const r = resolver();
    renderComponent(withData(<SourceRef dataRef="dv_aapl_net_sales_growth_fy2024" />, r));
    expect([...r.usedRefs]).toContain("dv_aapl_net_sales_growth_fy2024");
    expect([...r.usedSources].sort()).toEqual(["src_aapl_10k_2024"]);
  });

  it("RefText fills tokens: popovers for values, labels for periods, plain spans when asked", () => {
    const { container } = renderComponent(
      withData(
        <p>
          <RefText text="Sales were {dp_aapl_net_sales_fy2024} in {pd_fy2024}" cite="plain" />
        </p>,
      ),
    );
    expect(container.querySelector("p")?.textContent).toBe("Sales were $391.0bn in fiscal 2024");
    expect(container.querySelector(".cited")).toHaveAttribute("data-ref", "dp_aapl_net_sales_fy2024");
  });
});

describe("FigureFrame", () => {
  it("puts the caption first, titles the figure, links sources and offers the table and downloads", () => {
    const r = resolver();
    const series = r.series("sr_aapl_net_sales_5y");
    if (!series.ok) throw new Error("fixture series missing");
    const { container } = renderComponent(
      <FigureFrame
        id="f1"
        title="Sales grew again"
        subtitle="Total net sales, US$ bn"
        sources={series.sources}
        table={<table aria-label="data" />}
        downloads={{ csv: "charts/f1.csv", json: "charts/f1.json" }}
      >
        <div>plot</div>
      </FigureFrame>,
    );
    const figure = container.querySelector("figure") as HTMLElement;
    expect(figure.firstElementChild?.tagName).toBe("FIGCAPTION");
    expect(screen.getByRole("figure", { name: "Sales grew again" })).toBe(figure);
    expect(screen.getByRole("heading", { level: 2, name: "Sales grew again" })).toBeInTheDocument();
    expect(figure.querySelector(".figure-frame__source")?.textContent).toBe(
      "Sources: Apple Inc., Form 10-K, fiscal 2022; Apple Inc., Form 10-K, fiscal 2024",
    );
    expect(screen.getByText("Show the data table").tagName).toBe("SUMMARY");
    expect(screen.getByRole("link", { name: "CSV" })).toHaveAttribute("href", "charts/f1.csv");
  });

  it("leaves out the source line and actions when there are none", () => {
    const { container } = renderComponent(
      <FigureFrame id="f2" title="Only a title">
        <div />
      </FigureFrame>,
    );
    expect(container.querySelector(".figure-frame__source, .figure-frame__actions")).toBeNull();
  });
});

describe("Rule, Icon, VisuallyHidden, Text", () => {
  it("rules and decorative icons are hidden from assistive technology; labelled icons are images", () => {
    const { container } = renderComponent(
      <div>
        <Rule kind="brand" />
        <Icon name="external" />
        <Icon name="search" label="Search" />
      </div>,
    );
    expect(container.querySelector(".rule--brand")).toHaveAttribute("aria-hidden", "true");
    expect(container.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByRole("img", { name: "Search" })).toBeInTheDocument();
  });

  it("visually hidden text stays in the accessible name", () => {
    renderComponent(
      <button type="button">
        CSV <VisuallyHidden>download of the chart data</VisuallyHidden>
      </button>,
    );
    expect(screen.getByRole("button", { name: "CSV download of the chart data" })).toBeInTheDocument();
  });

  it("Text renders the element asked for with the variant's class", () => {
    renderComponent(
      <Text as="h1" variant="headline-xl" tabular>
        Headline
      </Text>,
    );
    expect(screen.getByRole("heading", { level: 1 })).toHaveClass("text-headline-xl", "text-tabular");
  });
});
