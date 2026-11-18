// Tester's tests for Popover and Tooltip (news-component-testing: rendering, content edge cases, server rendering,
// accessibility). Opening, closing, positioning and the screen matrix run in real browsers:
// tests/e2e/components/Popover.spec.ts.
import { screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { hydrateAndCollectWarnings } from "../../../tests/support/hydrate";
import { expectNoAxeViolations, renderComponent } from "../../../tests/support/render";
import { Popover, Tooltip } from "./Popover";

const popover = (props: Partial<Parameters<typeof Popover>[0]> = {}) => (
  <Popover id="p1" label="Source for $391.0bn" trigger="$391.0bn" {...props}>
    {props.children ?? <span className="popover__value">$391,035 million</span>}
  </Popover>
);

describe("Popover: native popover, no script", () => {
  it("is a type=button trigger wired to an auto popover panel by id", () => {
    const { container } = renderComponent(<p>{popover()}</p>);
    const button = screen.getByRole("button", { name: "$391.0bn" });
    expect(button).toHaveAttribute("type", "button");
    expect(button).toHaveClass("popover-trigger");
    expect(button).toHaveAttribute("popovertarget", "p1");
    const panel = container.querySelector("#p1") as HTMLElement;
    expect(panel.tagName).toBe("SPAN");
    expect(panel).toHaveAttribute("popover", "auto");
    expect(panel).toHaveAttribute("role", "dialog");
    expect(panel).toHaveAttribute("aria-label", "Source for $391.0bn");
    expect(panel).toHaveClass("popover");
    // Right after its trigger, so Tab moves from the trigger into the open panel.
    expect(button.nextElementSibling).toBe(panel);
  });

  it("adds the trigger class it is given, and keeps rich trigger content", () => {
    renderComponent(
      <p>
        {popover({
          triggerClassName: "source-ref text-figure",
          trigger: (
            <>
              <strong>$391.0bn</strong>
            </>
          ),
        })}
      </p>,
    );
    const button = screen.getByRole("button", { name: "$391.0bn" });
    expect(button).toHaveClass("popover-trigger", "source-ref", "text-figure");
    expect(button.querySelector("strong")).not.toBeNull();
  });

  it("is phrasing content end to end, so it can sit inside a sentence, a heading or a <dd>", () => {
    const html = renderToStaticMarkup(
      popover({
        children: (
          <>
            <span className="popover__value">v</span>
            <a className="popover__link" href="https://example.org">
              Open the source
            </a>
          </>
        ),
      }),
    );
    expect(html).not.toMatch(/<(div|p|section|ul|ol|table|h\d)[\s>]/);
  });

  it("two popovers on a page stay separate", () => {
    const { container } = renderComponent(
      <p>
        {popover({ id: "a", trigger: "one" })} and {popover({ id: "b", trigger: "two", label: "Source for two" })}
      </p>,
    );
    expect(screen.getByRole("button", { name: "one" })).toHaveAttribute("popovertarget", "a");
    expect(screen.getByRole("button", { name: "two" })).toHaveAttribute("popovertarget", "b");
    expect(container.querySelectorAll("[popover]")).toHaveLength(2);
  });

  it("has no axe violations in light and dark, open or closed content", async () => {
    for (const theme of ["light", "dark"] as const) {
      const { container, unmount } = renderComponent(<p>Net sales were {popover()} in fiscal 2024.</p>, { theme });
      await expectNoAxeViolations(container);
      unmount();
    }
  });

  it("server HTML holds the whole panel (readable without script) and hydrates with no warnings", async () => {
    const { html, warnings } = await hydrateAndCollectWarnings(<p>{popover()}</p>);
    // React writes the attribute as popoverTarget; HTML attribute names are case-insensitive.
    expect(html.toLowerCase()).toContain('popovertarget="p1"');
    expect(html).toContain("$391,035 million");
    expect(html).not.toContain("<script");
    expect(html).not.toContain("style=");
    expect(warnings).toEqual([]);
  });
});

describe("Tooltip: the same look for islands", () => {
  it("defaults: above, centred, positioned by left/top in px, announced politely", () => {
    const { container } = renderComponent(
      <Tooltip x={120} y={96}>
        $391.0bn
      </Tooltip>,
    );
    const tip = container.querySelector(".popover--tooltip") as HTMLElement;
    expect(tip).toHaveClass("popover", "popover--above", "popover--align-center");
    expect(tip).toHaveAttribute("role", "status");
    expect(tip.style.cssText).toBe("left: 120px; top: 96px;");
    expect(tip).not.toHaveAttribute("popover");
    expect(tip).not.toHaveAttribute("id");
  });

  it.each([
    ["below", "start"],
    ["below", "end"],
    ["above", "end"],
    ["above", "start"],
  ] as const)("placement %s, align %s", (placement, align) => {
    const { container } = renderComponent(
      <Tooltip id="t" x="40%" y="0" placement={placement} align={align}>
        v
      </Tooltip>,
    );
    const tip = container.querySelector("#t") as HTMLElement;
    expect(tip).toHaveClass(`popover--${placement}`, `popover--align-${align}`);
    // CSS lengths pass through untouched; the only inline style is the computed geometry.
    expect([tip.style.left, tip.style.top]).toEqual(["40%", "0px"]);
  });

  it("negative and fractional coordinates are written as given (the island clamps, not the tooltip)", () => {
    const { container } = renderComponent(
      <Tooltip x={-3.5} y={0}>
        v
      </Tooltip>,
    );
    const tip = container.querySelector(".popover--tooltip") as HTMLElement;
    expect([tip.style.left, tip.style.top]).toEqual(["-3.5px", "0px"]);
  });

  it("server-renders and hydrates with no warnings, and passes axe", async () => {
    const ui = (
      <div style={{ position: "relative" }}>
        <Tooltip x={10} y={10}>
          <span className="popover__value">$391.0bn</span>
          <span className="popover__line">Net sales, fiscal 2024</span>
        </Tooltip>
      </div>
    );
    const { warnings } = await hydrateAndCollectWarnings(ui);
    expect(warnings).toEqual([]);
    const { container } = renderComponent(<main>{ui}</main>);
    await expectNoAxeViolations(container);
  });
});
