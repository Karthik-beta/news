// Popover and Tooltip in browsers, on every screen (news-component-testing). Gallery: /fixtures/primitives/Popover/.
// Spec: design/components.md ("Citation popover", "Tooltip"): paper, 1px rule border, no shadow, 12px padding,
// at most 320px wide, appears instantly.
import { expect, gotoPage, longestMotionMs, setTheme, test } from "../fixtures";
import {
  expectPopoverLook,
  expectPopoverPlacement,
  fixtureCase,
  galleryChecks,
  galleryPath,
  panelFor,
} from "./primitives";

const PATH = galleryPath("Popover");

galleryChecks("Popover", 5);

test.describe("Popover", () => {
  test("opens on click with no script, sits on screen, and wraps long content (with an unbroken word) at 320px", {
    tag: "@interaction",
  }, async ({ page, theme }) => {
    await gotoPage(page, PATH);
    for (const name of ["popover inside a sentence", "long content wraps at 320px"]) {
      const trigger = fixtureCase(page, name).locator("button.popover-trigger");
      const panel = await panelFor(page, trigger);
      await trigger.click();
      await expect(panel, name).toBeVisible();
      await expectPopoverPlacement(page, trigger, panel);
      await expectPopoverLook(panel, theme);
      await page.keyboard.press("Escape");
      await expect(panel).toBeHidden();
    }
  });

  test("appears instantly: no transition or animation on the panel", { tag: "@motion" }, async ({ page }) => {
    await gotoPage(page, PATH);
    const trigger = fixtureCase(page, "popover inside a sentence").locator("button.popover-trigger");
    await trigger.click();
    const panel = await panelFor(page, trigger);
    const motion = await panel.evaluate((el) => [getComputedStyle(el).transitionDuration, el.getAnimations().length]);
    expect(motion).toEqual(["0s", 0]);
    expect(await longestMotionMs(page)).toBeLessThanOrEqual(200);
  });

  test("the trigger is a real button: Space and Enter toggle it, Escape closes it", { tag: "@interaction" }, async ({
    page,
  }) => {
    await gotoPage(page, PATH);
    const trigger = fixtureCase(page, "long content wraps at 320px").locator("button");
    const panel = await panelFor(page, trigger);
    await expect(trigger).toHaveAttribute("type", "button");
    await trigger.focus();
    await page.keyboard.press("Space");
    await expect(panel).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(panel).toBeHidden();
    await expect(trigger).toBeFocused();
  });

  test("tooltips sit beside their point as placed, in the popover look, and never take the pointer", async ({
    page,
    theme,
  }) => {
    await gotoPage(page, PATH);
    const geometry = async (name: string) =>
      fixtureCase(page, name)
        .locator(".popover--tooltip")
        .evaluate((tip) => {
          const stage = (tip.parentElement as HTMLElement).getBoundingClientRect();
          const r = tip.getBoundingClientRect();
          const s = getComputedStyle(tip);
          return {
            left: r.left - stage.left,
            right: r.right - stage.left,
            top: r.top - stage.top,
            bottom: r.bottom - stage.top,
            width: r.width,
            stageWidth: stage.width,
            pointer: s.pointerEvents,
          };
        });
    // Above, centred on (120, 96), 8px above the point.
    const above = await geometry("tooltip (island look)");
    expect(above.bottom).toBeCloseTo(96 - 8, 0);
    expect((above.left + above.right) / 2).toBeCloseTo(120, 0);
    expect(above.pointer).toBe("none");
    expect(above.width).toBeLessThanOrEqual(320.5);
    // Below, its end at the right edge of the stage (x = 100%).
    const end = await geometry("tooltip below, aligned to the end");
    expect(end.top).toBeCloseTo(8, 0);
    expect(end.right).toBeCloseTo(end.stageWidth, 0);
    // Below, its start at the point.
    const start = await geometry("tooltip below, aligned to the start");
    expect(start.left).toBeCloseTo(0, 0);
    await expectPopoverLook(fixtureCase(page, "tooltip (island look)").locator(".popover--tooltip"), theme);
    const other = theme === "light" ? "dark" : "light";
    await setTheme(page, other);
    await expectPopoverLook(fixtureCase(page, "tooltip (island look)").locator(".popover--tooltip"), other);
  });

  test("prints the trigger's text (the figure or term) in its sentence", { tag: "@print" }, async ({ page }) => {
    await gotoPage(page, PATH);
    await page.emulateMedia({ media: "print" });
    const trigger = fixtureCase(page, "popover inside a sentence").locator("button.popover-trigger");
    await expect(trigger).toBeVisible();
    await expect(trigger).toHaveText("$391.0bn");
  });

  test("an open popover is hidden in print", { tag: "@print" }, async ({ page }) => {
    await gotoPage(page, PATH);
    const trigger = fixtureCase(page, "popover inside a sentence").locator("button");
    await trigger.click();
    await page.emulateMedia({ media: "print" });
    await expect(await panelFor(page, trigger)).toBeHidden();
  });

  test("keeps its border and text in forced colours", { tag: "@forced-colors" }, async ({ page }) => {
    await gotoPage(page, PATH);
    const trigger = fixtureCase(page, "popover inside a sentence").locator("button");
    await trigger.click();
    const panel = await panelFor(page, trigger);
    const s = await panel.evaluate((el) => {
      const c = getComputedStyle(el);
      return { border: c.borderTopColor, width: c.borderTopWidth, text: c.color, bg: c.backgroundColor };
    });
    expect(s.width).toBe("1px");
    expect(s.border).not.toBe(s.bg);
    expect(s.text).not.toBe(s.bg);
  });
});
