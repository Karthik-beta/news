// Tester's tests for Rule (news-component-testing: rendering, accessibility, design-system compliance). Drawn
// widths, colours per theme, print and forced colours: tests/e2e/components/Rule.spec.ts.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { hydrateAndCollectWarnings } from "../../../tests/support/hydrate";
import { expectNoAxeViolations, renderComponent } from "../../../tests/support/render";
import { siteRoot } from "../../../tests/support/tokens";
import { Rule, type RuleKind, ruleTop } from "./Rule";

const KINDS: RuleKind[] = ["hairline", "ink", "section", "brand"];
const css = readFileSync(join(siteRoot, "src/components/Rule/Rule.css"), "utf8");

describe("Rule", () => {
  it("defaults to a hairline, hidden from assistive technology", () => {
    const { container } = renderComponent(<Rule />);
    expect(container.innerHTML).toBe('<div class="rule rule--hairline" aria-hidden="true"></div>');
  });

  it.each(KINDS)("kind %s has its modifier class and an extra class when given", (kind) => {
    const { container } = renderComponent(<Rule kind={kind} className="x" />);
    expect(container.firstElementChild?.className).toBe(`rule rule--${kind} x`);
    expect(container.firstElementChild).toHaveAttribute("aria-hidden", "true");
  });

  it.each(KINDS)("ruleTop(%s) gives the classes for a rule on top of any element", (kind) => {
    expect(ruleTop(kind)).toBe(`rule-top rule-top--${kind}`);
  });

  it("adds nothing to the accessibility tree and passes axe", async () => {
    const { container } = renderComponent(
      <main>
        {KINDS.map((k) => (
          <Rule key={k} kind={k} />
        ))}
        <div className={ruleTop("brand")}>
          <p>Block</p>
        </div>
      </main>,
    );
    await expectNoAxeViolations(container);
    expect(container.querySelectorAll('[aria-hidden="true"]')).toHaveLength(4);
  });

  it("server-renders and hydrates with no warnings", async () => {
    const { warnings } = await hydrateAndCollectWarnings(<Rule kind="brand" />);
    expect(warnings).toEqual([]);
  });

  it("draws with tokens only: rule, ink and brand colours, hairline and section widths, the 36 x 7 tag", () => {
    expect(css).toMatch(/\.rule,\s*\.rule-top\s*\{\s*border-top:\s*var\(--border-hairline\) solid var\(--color-rule\)/);
    expect(css).toMatch(/border-top-width:\s*var\(--border-section\)/);
    expect(css).toMatch(/width:\s*var\(--border-chart-tag-width\)/);
    expect(css).toMatch(/height:\s*var\(--border-chart-tag-height\)/);
    expect(css).toMatch(/background:\s*var\(--color-brand\)/);
    expect(css.replace(/\/\*[\s\S]*?\*\//g, "")).not.toMatch(/\d+px|#[0-9a-f]{3,8}\b|box-shadow|border-radius/i);
  });
});
