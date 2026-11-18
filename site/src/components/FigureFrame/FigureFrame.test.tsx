// Tester's tests for FigureFrame (news-component-testing: rendering, data, content edge cases, server rendering,
// accessibility). Geometry, themes, print and the screen matrix are in tests/e2e/components/FigureFrame.spec.ts.
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { TEST_SOURCE } from "../../../tests/support/data";
import { hydrateAndCollectWarnings } from "../../../tests/support/hydrate";
import { expectNoAxeViolations, renderComponent } from "../../../tests/support/render";
import type { CitedSource } from "../../renderer/data/resolve";
import { FigureFrame, sourceAnchor } from "./FigureFrame";

const one: CitedSource[] = [
  { id: "src_one", source: { ...TEST_SOURCE, publisher: "Apple Inc.", short: "Form 10-K, fiscal 2024" } },
];
const two: CitedSource[] = [
  ...one,
  { id: "src_two", source: { ...TEST_SOURCE, publisher: "Apple Inc.", short: "Form 10-K, fiscal 2022" } },
];
const table = (
  <table>
    <thead>
      <tr>
        <th scope="col">Year</th>
        <th scope="col" className="is-number">
          Value
        </th>
      </tr>
    </thead>
    <tbody>
      <tr>
        <th scope="row">2024</th>
        <td className="is-number">1.0</td>
      </tr>
    </tbody>
  </table>
);

const frame = (props: Partial<Parameters<typeof FigureFrame>[0]> = {}) => (
  <main>
    <FigureFrame id="f" title="Sales grew again" {...props}>
      {props.children ?? <div data-testid="plot">plot</div>}
    </FigureFrame>
  </main>
);

describe("FigureFrame: rendering every prop and default", () => {
  it("defaults: an h2 title that names the figure, the brand rule on top, the plot, nothing else", () => {
    const { container } = renderComponent(frame());
    const figure = container.querySelector("figure") as HTMLElement;
    expect(figure).toHaveClass("figure-frame", "rule-top", "rule-top--brand");
    expect(figure.firstElementChild?.tagName).toBe("FIGCAPTION");
    expect(screen.getByRole("heading", { level: 2, name: "Sales grew again" })).toHaveAttribute("id", "f-title");
    expect(screen.getByRole("figure", { name: "Sales grew again" })).toBe(figure);
    expect(figure.querySelector(".figure-frame__plot")?.contains(screen.getByTestId("plot"))).toBe(true);
    for (const part of ["subtitle", "legend", "source", "actions"]) {
      expect(figure.querySelector(`.figure-frame__${part}`), part).toBeNull();
    }
  });

  it.each([2, 3, 4] as const)("titles with an h%i when asked", (level) => {
    renderComponent(frame({ headingLevel: level }));
    expect(screen.getByRole("heading", { level, name: "Sales grew again" })).toHaveClass("text-chart-title");
  });

  it("writes the subtitle in the caption and the legend above the plot, in that order", () => {
    const { container } = renderComponent(
      frame({ subtitle: "Net sales, US$ bn", legend: <p className="text-label">Services, Products</p> }),
    );
    const caption = container.querySelector("figcaption") as HTMLElement;
    expect(caption.querySelector(".figure-frame__subtitle")).toHaveTextContent("Net sales, US$ bn");
    expect(caption.querySelector(".figure-frame__subtitle")).toHaveClass("text-chart-subtitle");
    const order = [...(container.querySelector("figure")?.children ?? [])].map((c) => c.className.split(" ")[0]);
    expect(order).toEqual(["figure-frame__caption", "figure-frame__legend", "figure-frame__plot"]);
  });

  it("one source: 'Source: ' and a link to its entry in the sources list", () => {
    const { container } = renderComponent(frame({ sources: one }));
    const line = container.querySelector(".figure-frame__source") as HTMLElement;
    expect(line).toHaveClass("text-source");
    expect(line.textContent).toBe("Source: Apple Inc., Form 10-K, fiscal 2024");
    expect(within(line).getByRole("link")).toHaveAttribute("href", `#${sourceAnchor("src_one")}`);
  });

  it("two sources: 'Sources: ' joined by semicolons, each linked", () => {
    const { container } = renderComponent(frame({ sources: two }));
    const line = container.querySelector(".figure-frame__source") as HTMLElement;
    expect(line.textContent).toBe("Sources: Apple Inc., Form 10-K, fiscal 2024; Apple Inc., Form 10-K, fiscal 2022");
    expect(
      within(line)
        .getAllByRole("link")
        .map((a) => a.getAttribute("href")),
    ).toEqual(["#source-src_one", "#source-src_two"]);
  });

  it("a source note comes before the sources; alone it still makes a source line", () => {
    const { container, unmount } = renderComponent(frame({ sources: one, sourceNote: "Calculated by News." }));
    expect(container.querySelector(".figure-frame__source")?.textContent).toBe(
      "Calculated by News. Source: Apple Inc., Form 10-K, fiscal 2024",
    );
    unmount();
    const alone = renderComponent(frame({ sourceNote: "Calculated by News." }));
    expect(alone.container.querySelector(".figure-frame__source")?.textContent?.trim()).toBe("Calculated by News.");
  });

  it("actions: the data table toggle, then CSV, then JSON, each only when given", () => {
    const { container, unmount } = renderComponent(
      frame({ table, downloads: { csv: "charts/f.csv", json: "charts/f.json" } }),
    );
    const actions = container.querySelector(".figure-frame__actions") as HTMLElement;
    expect([...actions.children].map((c) => c.textContent?.slice(0, 19))).toEqual([
      "Show the data table",
      "CSV",
      "JSON",
    ]);
    expect(within(actions).getByRole("link", { name: "CSV" })).toHaveAttribute("download");
    expect(within(actions).getByRole("link", { name: "JSON" })).toHaveAttribute("href", "charts/f.json");
    unmount();

    for (const [downloads, links] of [
      [{ csv: "a.csv" }, ["CSV"]],
      [{ json: "a.json" }, ["JSON"]],
      [{}, []],
    ] as const) {
      const r = renderComponent(frame({ downloads }));
      const names = [...r.container.querySelectorAll(".figure-frame__actions a")].map((a) => a.textContent);
      expect(names).toEqual(links);
      if (links.length === 0) expect(r.container.querySelector(".figure-frame__actions")).toBeNull();
      r.unmount();
    }
  });

  // Keyboard (Enter, Space on the summary) needs a real browser: tests/e2e/components/FigureFrame.spec.ts.
  it("the data table is a closed native <details> that opens and closes from its summary", async () => {
    const user = userEvent.setup();
    const { container } = renderComponent(frame({ table }));
    const details = container.querySelector("details.figure-frame__table") as HTMLDetailsElement;
    expect(details.open).toBe(false);
    const summary = screen.getByText("Show the data table");
    expect(summary.tagName).toBe("SUMMARY");
    expect(details.querySelector(".figure-frame__table-scroll table")).not.toBeNull();
    await user.click(summary);
    expect(details.open).toBe(true);
    await user.click(summary);
    expect(details.open).toBe(false);
  });
});

describe("FigureFrame: content edge cases", () => {
  it("keeps a very long title and subtitle whole (no truncation)", () => {
    const title = "The International Monetary Fund lowered its forecast for global growth ".repeat(3).trim();
    renderComponent(frame({ title, subtitle: "x".repeat(400) }));
    expect(screen.getByRole("heading").textContent).toBe(title);
  });

  it("takes rich titles (a cited figure inside) and keeps the accessible name", () => {
    renderComponent(
      frame({
        title: (
          <>
            Sales reached <span className="cited">$391.0bn</span>
          </>
        ),
      }),
    );
    expect(screen.getByRole("figure", { name: "Sales reached $391.0bn" })).toBeInTheDocument();
  });

  it("two frames on a page keep unique title ids and names", () => {
    const { container } = renderComponent(
      <main>
        <FigureFrame id="a" title="First">
          <div />
        </FigureFrame>
        <FigureFrame id="b" title="Second">
          <div />
        </FigureFrame>
      </main>,
    );
    const ids = [...container.querySelectorAll("[id]")].map((e) => e.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(screen.getAllByRole("figure").map((f) => f.getAttribute("aria-labelledby"))).toEqual(["a-title", "b-title"]);
  });

  it("an empty sources list draws no source line", () => {
    const { container } = renderComponent(frame({ sources: [] }));
    expect(container.querySelector(".figure-frame__source")).toBeNull();
  });
});

describe("FigureFrame: accessibility and server rendering", () => {
  it("full anatomy has no axe violations in light and dark", async () => {
    for (const theme of ["light", "dark"] as const) {
      const { container, unmount } = renderComponent(
        frame({
          subtitle: "Net sales, US$ bn",
          legend: <p>Key</p>,
          sources: two,
          sourceNote: "Calculated by News.",
          table,
          downloads: { csv: "a.csv", json: "a.json" },
        }),
        { theme },
      );
      await expectNoAxeViolations(container);
      unmount();
    }
  });

  it("server HTML is complete (title, source, table, links) and hydrates with no warnings", async () => {
    const { html, warnings } = await hydrateAndCollectWarnings(
      <FigureFrame id="h" title="Hydrated" subtitle="Sub" sources={one} table={table} downloads={{ csv: "a.csv" }}>
        <svg aria-hidden="true" />
      </FigureFrame>,
    );
    expect(html).toContain('<h2 class="figure-frame__title text-chart-title" id="h-title">Hydrated</h2>');
    expect(html).toContain("<summary>Show the data table</summary>");
    expect(html).toContain('href="#source-src_one"');
    expect(html).not.toContain("<script");
    expect(warnings).toEqual([]);
  });
});
