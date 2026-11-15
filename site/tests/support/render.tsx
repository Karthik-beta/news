// Shared helpers for React component tests (Vitest "component" project, jsdom).

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { type RenderOptions, type RenderResult, render } from "@testing-library/react";
import axe from "axe-core";
import type { ReactElement } from "react";
import { siteRoot } from "./tokens";

export type Theme = "light" | "dark";

const tokensCss = readFileSync(join(siteRoot, "src/styles/tokens.css"), "utf8");

/** WCAG 2.2 A and AA rule tags: the target in design/accessibility.md. */
export const WCAG_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

/**
 * Renders a component with the design tokens loaded and the theme set through `data-theme`, the same
 * attribute the site's theme switch uses. jsdom has no layout, so test geometry and contrast in Playwright.
 */
export function renderComponent(
  ui: ReactElement,
  { theme = "light", ...options }: { theme?: Theme } & Omit<RenderOptions, "queries"> = {},
): RenderResult {
  let style = document.getElementById("news-tokens");
  if (!style) {
    style = document.createElement("style");
    style.id = "news-tokens";
    style.textContent = tokensCss;
    document.head.append(style);
  }
  document.documentElement.setAttribute("data-theme", theme);
  document.documentElement.lang = "en-GB";
  return render(ui, options);
}

/** Value of a design token as the browser resolves it on <html> for the current theme. */
export function cssToken(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/**
 * Runs axe-core against a rendered container with the WCAG 2.2 AA rules and fails on any violation.
 * Colour contrast needs real layout and is checked in Playwright instead.
 */
export async function expectNoAxeViolations(container: Element, disableRules: string[] = []): Promise<void> {
  const results = await axe.run(container, {
    runOnly: { type: "tag", values: WCAG_TAGS },
    rules: Object.fromEntries(["color-contrast", ...disableRules].map((id) => [id, { enabled: false }])),
  });
  const report = results.violations.map(
    (v) => `${v.impact ?? "?"} ${v.id}: ${v.help}\n  ${v.nodes.map((n) => n.target.join(" ")).join("\n  ")}`,
  );
  if (report.length > 0) throw new Error(`axe found ${report.length} violation(s):\n${report.join("\n")}`);
}
