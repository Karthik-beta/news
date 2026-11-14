import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test("site shell renders, has no serious accessibility issues and no sideways scroll", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();

  const sideways = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
  expect(sideways).toBe(false);

  const { violations } = await new AxeBuilder({ page }).analyze();
  expect(violations.filter((v) => v.impact === "serious" || v.impact === "critical")).toEqual([]);
});
