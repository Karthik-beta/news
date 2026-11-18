// Tester's tests for Icon (news-component-testing: rendering, accessibility, design-system compliance). Size,
// colour and crispness in browsers: tests/e2e/components/Icon.spec.ts.
import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { hydrateAndCollectWarnings } from "../../../tests/support/hydrate";
import { expectNoAxeViolations, renderComponent } from "../../../tests/support/render";
import { ICON_NAMES, Icon } from "./Icon";

/** The points an SVG path visits (the commands these icons use: M L H V Z, absolute and relative). */
function pathPoints(d: string): number[] {
  const out: number[] = [];
  let x = 0;
  let y = 0;
  for (const [, cmd, args] of d.matchAll(/([MLHVZmlhvz])([^MLHVZmlhvz]*)/g)) {
    const n = [...(args as string).matchAll(/-?\d*\.?\d+/g)].map((m) => Number(m[0]));
    const rel = cmd === (cmd as string).toLowerCase();
    const c = (cmd as string).toUpperCase();
    if (c === "H") for (const v of n) x = rel ? x + v : v;
    else if (c === "V") for (const v of n) y = rel ? y + v : v;
    else if (c === "M" || c === "L")
      for (let i = 0; i + 1 < n.length; i += 2) {
        x = rel ? x + (n[i] as number) : (n[i] as number);
        y = rel ? y + (n[i + 1] as number) : (n[i + 1] as number);
        out.push(x, y);
      }
    if (c === "H" || c === "V") out.push(x, y);
    if (c === "Z") continue;
    if (!/[MLHVZ]/.test(c)) throw new Error(`unexpected path command ${cmd}`);
  }
  return out;
}

/** Every number in an SVG path or shape, to check the drawing stays on the 16-unit grid. */
function coordinates(svg: SVGElement): number[] {
  const out: number[] = [];
  for (const el of svg.querySelectorAll("path, circle")) {
    if (el.tagName === "circle") {
      const [cx, cy, r] = ["cx", "cy", "r"].map((a) => Number(el.getAttribute(a)));
      out.push(
        (cx as number) - (r as number),
        (cx as number) + (r as number),
        (cy as number) - (r as number),
        (cy as number) + (r as number),
      );
    } else {
      out.push(...pathPoints(el.getAttribute("d") ?? ""));
    }
  }
  return out;
}

describe("Icon", () => {
  it("has exactly the eight icons of design/components.md", () => {
    expect([...ICON_NAMES]).toEqual(["search", "external", "download", "close", "play", "error", "check", "info"]);
  });

  it.each(ICON_NAMES)("%s: an inline SVG on the 16-unit grid, decorative by default", (name) => {
    const { container } = renderComponent(<Icon name={name} />);
    const svg = container.querySelector("svg") as SVGElement;
    expect(svg).toHaveAttribute("viewBox", "0 0 16 16");
    expect(svg).toHaveAttribute("aria-hidden", "true");
    expect(svg).toHaveAttribute("focusable", "false");
    expect(svg).not.toHaveAttribute("role");
    expect(svg).toHaveClass("icon");
    expect(svg.querySelectorAll("path, circle").length).toBeGreaterThan(0);
    for (const n of coordinates(svg)) {
      expect(n, `${name} stays inside the grid`).toBeGreaterThanOrEqual(0);
      expect(n, `${name} stays inside the grid`).toBeLessThanOrEqual(16);
    }
    // No colour is baked in: stroke and fill come from currentColor in Icon.css.
    expect(container.innerHTML).not.toMatch(/(fill|stroke)="(?!none)/);
  });

  it.each(ICON_NAMES)("%s with a label is an image with that name", (name) => {
    renderComponent(<Icon name={name} label={`Label ${name}`} />);
    const img = screen.getByRole("img", { name: `Label ${name}` });
    expect(img).not.toHaveAttribute("aria-hidden");
  });

  it("only play, error and info have filled parts", () => {
    const filled = ICON_NAMES.filter((name) => {
      const { container, unmount } = renderComponent(<Icon name={name} />);
      const has = container.querySelector(".icon__fill") !== null;
      unmount();
      return has;
    });
    expect(filled).toEqual(["play", "error", "info"]);
  });

  it("adds an extra class when given", () => {
    const { container } = renderComponent(<Icon name="close" className="x" />);
    expect(container.querySelector("svg")).toHaveAttribute("class", "icon x");
  });

  it("a labelled icon alone in a button names the button; a decorative one adds nothing to a link's name", async () => {
    const { container } = renderComponent(
      <main>
        <button type="button">
          <Icon name="close" label="Close" />
        </button>
        <a href="https://example.org">
          Open the source <Icon name="external" />
        </a>
      </main>,
    );
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open the source" })).toBeInTheDocument();
    await expectNoAxeViolations(container);
  });

  it("server-renders and hydrates with no warnings", async () => {
    const { warnings } = await hydrateAndCollectWarnings(
      <p>
        {ICON_NAMES.map((n) => (
          <Icon key={n} name={n} />
        ))}
        <Icon name="search" label="Search" />
      </p>,
    );
    expect(warnings).toEqual([]);
  });
});
