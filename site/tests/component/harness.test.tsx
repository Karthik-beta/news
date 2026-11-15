// There are no React components yet (they arrive in Phase 3). Until the first one lands, this file tests the
// component-test helpers themselves against tokens.json, so a broken helper cannot pass a broken component.
// Component tests live next to the component (src/components/<Name>/<Name>.test.tsx) or in tests/component/.
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { cssToken, expectNoAxeViolations, renderComponent } from "../support/render";
import { tokenValue } from "../support/tokens";

function Disclosure() {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button type="button" aria-expanded={open} aria-controls="panel" onClick={() => setOpen(!open)}>
        Show the data table
      </button>
      <div id="panel" hidden={!open}>
        Revenue, $bn
      </div>
    </div>
  );
}

describe("component test helpers", () => {
  it.each(["light", "dark"] as const)("load the design tokens for the %s theme", (theme) => {
    renderComponent(<p>Text</p>, { theme });
    for (const name of ["--color-paper", "--color-ink", "--color-brand", "--chart-highlight"]) {
      expect(cssToken(name)).toBe(tokenValue(name, theme));
    }
    expect(document.documentElement).toHaveAttribute("data-theme", theme);
  });

  it("drive a component with the keyboard through user-event, with jest-dom matchers", async () => {
    const user = userEvent.setup();
    renderComponent(<Disclosure />);
    const button = screen.getByRole("button", { name: "Show the data table" });
    expect(screen.getByText("Revenue, $bn")).not.toBeVisible();

    await user.tab();
    expect(button).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Revenue, $bn")).toBeVisible();
  });

  it("pass axe on an accessible tree", async () => {
    const { container } = renderComponent(<Disclosure />);
    await expectNoAxeViolations(container);
  });

  it("fail axe on a real WCAG violation", async () => {
    const { container } = renderComponent(
      <div>
        {/* biome-ignore lint/a11y/useAltText: the missing alt is the violation this test needs axe to catch */}
        <img src="chart.png" />
        <button type="button" />
      </div>,
    );
    const error = await expectNoAxeViolations(container).then(
      () => new Error("expected axe to report violations"),
      (e: Error) => e,
    );
    expect(error.message).toMatch(/image-alt/);
    expect(error.message).toMatch(/button-name/);
  });
});
