// Tester's tests for VisuallyHidden (news-component-testing: rendering, accessibility). Whether it is really
// invisible, and shows again on focus, is measured in browsers: tests/e2e/components/VisuallyHidden.spec.ts.
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { hydrateAndCollectWarnings } from "../../../tests/support/hydrate";
import { expectNoAxeViolations, renderComponent } from "../../../tests/support/render";
import { VisuallyHidden } from "./VisuallyHidden";

describe("VisuallyHidden", () => {
  it("defaults to a span with the hidden class", () => {
    const { container } = renderComponent(<VisuallyHidden>Source: </VisuallyHidden>);
    expect(container.innerHTML).toBe('<span class="visually-hidden">Source: </span>');
  });

  it("renders the element asked for, with an id, and the focusable modifier for skip links", () => {
    const { container } = renderComponent(
      <VisuallyHidden as="div" id="skip" focusable>
        <a href="#main">Skip to content</a>
      </VisuallyHidden>,
    );
    const el = container.querySelector("#skip") as HTMLElement;
    expect(el.tagName).toBe("DIV");
    expect(el.className).toBe("visually-hidden visually-hidden--focusable");
  });

  it("stays in the accessible name and the accessibility tree (no aria-hidden, no display:none)", () => {
    renderComponent(
      <a href="charts/a.csv">
        CSV <VisuallyHidden>download of the chart data</VisuallyHidden>
      </a>,
    );
    const link = screen.getByRole("link", { name: "CSV download of the chart data" });
    expect(link.querySelector(".visually-hidden")).not.toHaveAttribute("aria-hidden");
  });

  it("a focusable skip link is reachable by Tab", async () => {
    const user = userEvent.setup();
    renderComponent(
      <>
        <VisuallyHidden focusable>
          <a href="#main">Skip to content</a>
        </VisuallyHidden>
        <main id="main">Main</main>
      </>,
    );
    await user.tab();
    expect(screen.getByRole("link", { name: "Skip to content" })).toHaveFocus();
  });

  it("passes axe and hydrates with no warnings", async () => {
    const ui = (
      <main>
        <p>
          Net sales <VisuallyHidden>in fiscal 2024 were</VisuallyHidden> $391.0bn
        </p>
      </main>
    );
    const { container } = renderComponent(ui);
    await expectNoAxeViolations(container);
    const { warnings } = await hydrateAndCollectWarnings(ui);
    expect(warnings).toEqual([]);
  });
});
