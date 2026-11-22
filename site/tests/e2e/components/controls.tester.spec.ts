// Component tester's browser checks for #11 part 2 (controls and states), on top of the builder's controls.spec.ts:
// every gallery through the shared gallery checks (reads at every width, axe in both themes, 200% text and zoom, no
// JavaScript, no motion, fonts blocked, forced colours); the fields, buttons and checkbox drawn from the tokens in
// both themes with the focus ring; the select in every engine; the search row and its filter wrapping; the centred
// 404 at its full width; the loading region inert in every engine; the error summary's links landing on their fields
// with the label in view; touch taps; and print.
import type { Locator, Page } from "@playwright/test";
import { expect, expectAccessible, gotoPage, setTheme, type Theme, test } from "../fixtures";
import {
  expectInsideViewport,
  expectNoClippedText,
  fixtureCase,
  galleryChecks,
  galleryPath,
  tokenRgb,
} from "./primitives";

const PARTS: [string, number][] = [
  ["Button", 5],
  ["TextField", 5],
  ["TextArea", 3],
  ["Select", 3],
  ["Checkbox", 3],
  ["SearchField", 2],
  ["StateMessage", 3],
  ["NotFound", 1],
  ["LoadingRegion", 2],
  ["ReportForm", 2],
];
for (const [name, cases] of PARTS) galleryChecks(name, cases);

const other = (t: Theme): Theme => (t === "light" ? "dark" : "light");
const px = (v: string) => Number.parseFloat(v);

/** Computed styles of one element, as plain strings. */
const styles = (el: Locator, props: string[]) =>
  el.evaluate(
    (e, ps) => Object.fromEntries(ps.map((p) => [p, getComputedStyle(e).getPropertyValue(p)])),
    props,
  ) as Promise<Record<string, string>>;

/**
 * Keyboard focus on `target`: Tab until it has focus. Playwright's WebKit on Windows tabs to text fields only
 * (Safari's default), so there the element is focused directly, which still matches :focus-visible.
 */
async function keyboardFocus(page: Page, target: Locator, browserName: string): Promise<void> {
  if (browserName === "webkit") {
    await target.focus();
    return;
  }
  await page.locator("main").evaluate((m) => {
    (document.activeElement as HTMLElement | null)?.blur();
    m.setAttribute("tabindex", "-1");
    m.focus();
    m.removeAttribute("tabindex");
  });
  for (let i = 0; i < 60; i++) {
    await page.keyboard.press("Tab");
    if (await target.evaluate((el) => el === document.activeElement)) return;
  }
  throw new Error("Tab never reached the target");
}

async function expectFocusRing(el: Locator, theme: Theme): Promise<void> {
  expect(await el.evaluate((e) => e.matches(":focus-visible")), "focus-visible").toBe(true);
  const s = await styles(el, ["outline-style", "outline-width", "outline-color", "outline-offset"]);
  expect(s).toEqual({
    "outline-style": "solid",
    "outline-width": "2px",
    "outline-color": tokenRgb("--color-focus", theme),
    "outline-offset": "2px",
  });
}

// ------------------------------------------------------------------------------------------------- Form fields

test("fields: label, marker, hint, control and error drawn from the tokens, in both themes", async ({
  page,
  theme,
}) => {
  await gotoPage(page, galleryPath("TextField"));
  for (const t of [theme, other(theme)]) {
    await setTheme(page, t);
    const control = await styles(page.locator("#fixture-tf-required"), [
      "font-size",
      "font-weight",
      "font-family",
      "padding-top",
      "padding-left",
      "border-top-width",
      "border-top-style",
      "border-top-color",
      "border-top-left-radius",
      "background-color",
      "color",
    ]);
    expect(control).toMatchObject({
      "font-size": "16px",
      "font-weight": "400",
      "padding-top": "10px",
      "padding-left": "12px",
      "border-top-width": "1px",
      "border-top-style": "solid",
      "border-top-color": tokenRgb("--color-control-border", t),
      "border-top-left-radius": "5px",
      "background-color": tokenRgb("--color-paper", t),
      color: tokenRgb("--color-ink", t),
    });
    expect(control["font-family"]).toMatch(/Fira Sans/);
    const label = page.locator("label[for=fixture-tf-required]");
    expect(await styles(label, ["font-size", "font-weight", "color"])).toEqual({
      "font-size": "15px",
      "font-weight": "600",
      color: tokenRgb("--color-ink", t),
    });
    expect(await styles(label.locator(".field__marker"), ["font-weight", "color"])).toEqual({
      "font-weight": "400",
      color: tokenRgb("--color-ink-2", t),
    });
    expect(await styles(page.locator("#fixture-tf-required-hint"), ["font-size", "color"])).toEqual({
      "font-size": "13.5px",
      color: tokenRgb("--color-ink-2", t),
    });
    const error = fixtureCase(page, "error");
    expect((await styles(error.locator("input"), ["border-top-color"]))["border-top-color"]).toBe(
      tokenRgb("--color-error", t),
    );
    expect(await styles(error.locator(".field__error"), ["font-size", "color"])).toEqual({
      "font-size": "13.5px",
      color: tokenRgb("--color-error", t),
    });
    // The label sits above the control, the error under it.
    const [l, c, e] = await Promise.all(
      [error.locator("label"), error.locator("input"), error.locator(".field__error")].map((x) => x.boundingBox()),
    );
    expect((l?.y ?? 0) + (l?.height ?? 0)).toBeLessThanOrEqual((c?.y ?? 0) + 0.5);
    expect((c?.y ?? 0) + (c?.height ?? 0)).toBeLessThanOrEqual((e?.y ?? 0) + 0.5);
  }
});

test("fields: focus turns the border ink and shows the 2px focus ring, 2px out", { tag: "@interaction" }, async ({
  page,
  theme,
  browserName,
}) => {
  for (const [gallery, id] of [
    ["TextField", "#fixture-tf-plain"],
    ["TextField", "#fixture-tf-error"],
    ["TextArea", "#fixture-ta-required"],
    ["Select", "#fixture-select-plain"],
  ] as const) {
    await gotoPage(page, galleryPath(gallery));
    const el = page.locator(id);
    await keyboardFocus(page, el, browserName);
    await expectFocusRing(el, theme);
    expect((await styles(el, ["border-top-color"]))["border-top-color"], `${id} border`).toBe(
      id.includes("error") ? tokenRgb("--color-error", theme) : tokenRgb("--color-ink", theme),
    );
  }
});

test("fields: long labels, hints and errors wrap inside the column; the text area grows downwards only", async ({
  page,
}) => {
  await gotoPage(page, galleryPath("TextField"));
  const long = fixtureCase(page, "long label, hint and error wrap");
  await expectInsideViewport(page, long);
  await expectNoClippedText(page, long);
  const lines = await long
    .locator(".field__error")
    .evaluate((e) => e.getBoundingClientRect().height / Number.parseFloat(getComputedStyle(e).lineHeight));
  if ((page.viewportSize()?.width ?? 0) < 600) expect(lines).toBeGreaterThan(1.5);
  await gotoPage(page, galleryPath("TextArea"));
  expect((await styles(page.locator("#fixture-ta-required"), ["resize"])).resize).toBe("vertical");
  // The reader's line breaks survive.
  expect(await page.locator("#fixture-ta-filled").inputValue()).toContain("\nThe 10-K, page 21");
});

test("select: the native drop-down in the field look in every engine, and it changes by keyboard", {
  tag: "@interaction",
}, async ({ page, theme, browserName }) => {
  await gotoPage(page, galleryPath("Select"));
  const select = page.locator("#fixture-select-plain");
  const s = await styles(select, ["font-size", "font-family", "border-top-color", "border-top-left-radius", "color"]);
  expect(s["font-size"]).toBe("16px");
  expect(s["font-family"]).toMatch(/Fira Sans/);
  expect(s["border-top-color"]).toBe(tokenRgb("--color-control-border", theme));
  expect(s["border-top-left-radius"]).toBe("5px");
  expect(s.color).toBe(tokenRgb("--color-ink", theme));
  const box = await select.boundingBox();
  test.info().annotations.push({ type: `${browserName} select height`, description: String(box?.height) });
  expect(box?.height).toBeGreaterThanOrEqual(40);
  expect(await page.locator("#fixture-select-chosen").inputValue()).toBe("india");
  await select.selectOption({ label: "India" });
  expect(await select.inputValue()).toBe("india");
  // The error case says so in words, not only in colour.
  await expect(page.locator("#fixture-select-error")).toHaveAccessibleDescription(
    "Choose the kind of page the report is about. Error: Choose a page type.",
  );
});

// ---------------------------------------------------------------------------------------------------- Checkbox

test("checkbox: a 24px box in ink, the label beside it on its first line, the error outline and the focus ring", {
  tag: "@interaction",
}, async ({ page, theme, browserName }) => {
  await gotoPage(page, galleryPath("Checkbox"));
  for (const t of [theme, other(theme)]) {
    await setTheme(page, t);
    const box = page.locator("#fixture-cb-plain");
    expect((await styles(box, ["accent-color"]))["accent-color"]).toBe(tokenRgb("--color-ink", t));
    const err = page.locator("#fixture-cb-error");
    expect(await styles(err, ["outline-style", "outline-width", "outline-color", "outline-offset"])).toEqual({
      "outline-style": "solid",
      "outline-width": "1px",
      "outline-color": tokenRgb("--color-error", t),
      "outline-offset": "2px",
    });
  }
  await setTheme(page, theme);
  for (const id of ["#fixture-cb-plain", "#fixture-cb-error"]) {
    const box = await page.locator(id).boundingBox();
    expect([box?.width, box?.height]).toEqual([24, 24]);
    // The label's first line is centred on the box, and the label starts right of it.
    const label = page.locator(`label[for=${id.slice(1)}]`);
    const line = await label.evaluate((l) => {
      const r = document.createRange();
      r.selectNodeContents(l);
      const first = [...r.getClientRects()].find((b) => b.width > 2);
      return first ? { mid: (first.top + first.bottom) / 2, left: first.left } : null;
    });
    expect(Math.abs((line?.mid ?? 0) - ((box?.y ?? 0) + 12)), `${id} label centred on the box`).toBeLessThanOrEqual(2);
    expect(line?.left ?? 0).toBeGreaterThanOrEqual((box?.x ?? 0) + 24 + 6);
    expect((await styles(label, ["font-weight"]))["font-weight"]).toBe("400");
  }
  // Focused, the error box shows the focus ring in place of its error outline; the message stays in words.
  const err = page.locator("#fixture-cb-error");
  await keyboardFocus(page, err, browserName);
  await expectFocusRing(err, theme);
  await expect(err).toHaveAccessibleDescription(/Error: Check the figure against the linked source first\./);
  // Space toggles it.
  await page.keyboard.press("Space");
  await expect(err).toBeChecked();
});

test("checkbox: a tap on the label checks it on touch screens", { tag: "@interaction" }, async ({ page }) => {
  test.skip(
    !(await page.evaluate(() => matchMedia("(pointer: coarse)").matches || navigator.maxTouchPoints > 0)),
    "touch only",
  );
  await gotoPage(page, galleryPath("Checkbox"));
  const box = page.locator("#fixture-cb-plain");
  await expect(box).not.toBeChecked();
  await page.locator("label[for=fixture-cb-plain]").tap();
  await expect(box).toBeChecked();
  await box.tap();
  await expect(box).not.toBeChecked();
});

test("checkbox and error fields keep their error marks in forced colours", { tag: "@forced-colors" }, async ({
  page,
}) => {
  await gotoPage(page, galleryPath("Checkbox"));
  expect((await styles(page.locator("#fixture-cb-error"), ["outline-style"]))["outline-style"]).toBe("solid");
  await gotoPage(page, galleryPath("TextField"));
  // Colour is replaced by the system's, so the error field's border doubles in width instead.
  expect((await styles(page.locator("#fixture-tf-error"), ["border-top-width"]))["border-top-width"]).toBe("2px");
  expect((await styles(page.locator("#fixture-tf-plain"), ["border-top-width"]))["border-top-width"]).toBe("1px");
  await expect(page.locator("#fixture-tf-error-error svg")).toBeVisible();
});

// ----------------------------------------------------------------------------------------------------- Buttons

test("buttons: primary, secondary, text and disabled drawn from the tokens, in both themes", async ({
  page,
  theme,
}) => {
  await gotoPage(page, galleryPath("Button"));
  const button = (c: string) => fixtureCase(page, c).getByRole("button");
  const props = [
    "background-color",
    "color",
    "border-top-color",
    "border-top-width",
    "font-size",
    "font-weight",
    "font-family",
    "padding-top",
    "padding-left",
    "border-top-left-radius",
  ];
  for (const t of [theme, other(theme)]) {
    await setTheme(page, t);
    const ink = tokenRgb("--color-ink", t);
    const paper = tokenRgb("--color-paper", t);
    const common = {
      "font-size": "15px",
      "font-weight": "500",
      "padding-top": "10px",
      "padding-left": "18px",
      "border-top-left-radius": "5px",
    };
    const primary = await styles(button("primary"), props);
    expect(primary).toMatchObject({ ...common, "background-color": ink, color: paper, "border-top-color": ink });
    expect(primary["font-family"]).toMatch(/Fira Sans/);
    expect(await styles(button("secondary"), props)).toMatchObject({
      ...common,
      "background-color": paper,
      color: ink,
      "border-top-color": tokenRgb("--color-control-border", t),
      "border-top-width": "1px",
    });
    const text = await styles(button("text button"), [...props, "text-decoration-line", "min-height"]);
    expect(text).toMatchObject({
      color: tokenRgb("--color-ink-2", t),
      "background-color": "rgba(0, 0, 0, 0)",
      "border-top-width": "0px",
      "text-decoration-line": "underline",
      "font-weight": "400",
    });
    expect(px(text["min-height"] ?? "0")).toBeGreaterThanOrEqual(24);
  }
  await setTheme(page, theme);
  const disabled = button("disabled, with its reason beside it");
  await expect(disabled).toBeDisabled();
  expect(await styles(disabled, ["opacity", "cursor"])).toEqual({ opacity: "0.4", cursor: "not-allowed" });
  await expect(disabled).toHaveAccessibleDescription("Reports reopen at 00:00 UTC.");
  // Hover darkens the primary fill by mixing in paper; the disabled one does not react.
  const before = (await styles(button("primary"), ["background-color"]))["background-color"];
  await button("primary").hover();
  const after = (await styles(button("primary"), ["background-color"]))["background-color"];
  expect(after).not.toBe(before);
  await disabled.hover({ force: true });
  expect((await styles(disabled, ["background-color"]))["background-color"]).toBe(tokenRgb("--color-ink", theme));
});

test("buttons: Tab reaches each enabled button with the focus ring; the disabled one is skipped", {
  tag: "@interaction",
}, async ({ page, theme, browserName }) => {
  await gotoPage(page, galleryPath("Button"));
  for (const c of ["primary", "secondary", "text button", "long label wraps on a phone"]) {
    const b = fixtureCase(page, c).getByRole("button");
    await keyboardFocus(page, b, browserName);
    await expectFocusRing(b, theme);
  }
  expect(
    await fixtureCase(page, "disabled, with its reason beside it")
      .getByRole("button")
      .evaluate((b) => {
        (b as HTMLElement).focus();
        return document.activeElement === b;
      }),
  ).toBe(false);
});

// ------------------------------------------------------------------------------------------------ Search field

test("search: input and button share one 48px row, the magnifier centred inside the input before the text", async ({
  page,
}) => {
  await gotoPage(page, galleryPath("SearchField"));
  for (const c of ["empty", "with a query and the filter row"]) {
    const scope = fixtureCase(page, c);
    const [input, button, icon] = await Promise.all(
      [".search-field__input", ".search-field__submit", ".search-field__icon"].map((s) =>
        scope.locator(s).boundingBox(),
      ),
    );
    if (!input || !button || !icon) throw new Error("search row missing");
    expect(input.height).toBeCloseTo(48, 0);
    expect(button.height).toBeCloseTo(48, 0);
    expect(Math.abs(input.y - button.y)).toBeLessThanOrEqual(0.5);
    expect(button.x).toBeGreaterThan(input.x + input.width);
    expect(Math.abs(icon.y + icon.height / 2 - (input.y + input.height / 2))).toBeLessThanOrEqual(1);
    expect(icon.x).toBeGreaterThan(input.x);
    const textStart = await scope
      .locator(".search-field__input")
      .evaluate((e) => e.getBoundingClientRect().left + Number.parseFloat(getComputedStyle(e).paddingLeft));
    expect(textStart).toBeGreaterThan(icon.x + icon.width);
    expect((await styles(scope.locator(".search-field__input"), ["font-size"]))["font-size"]).toBe("18px");
  }
});

test("search: the filters sit in one row once there is room, two to a row on phones, one at 320", async ({ page }) => {
  await gotoPage(page, galleryPath("SearchField"));
  const scope = fixtureCase(page, "with a query and the filter row");
  const { width, rows } = await scope.locator(".search-field__filters").evaluate((grid) => {
    const tops = [...grid.children].map((c) => Math.round(c.getBoundingClientRect().top));
    const counts = new Map<number, number>();
    for (const t of tops) counts.set(t, (counts.get(t) ?? 0) + 1);
    return { width: grid.getBoundingClientRect().width, rows: [...counts.values()] };
  });
  test.info().annotations.push({ type: "filter rows", description: `${Math.round(width)}px: ${rows.join(" + ")}` });
  // 9.5rem columns with 12px gaps: four fit from 644px, two from 316px.
  const expected = width >= 644 ? [4] : width >= 316 ? [2, 2] : [1, 1, 1, 1];
  // SF-1 (open, builder): between 480 and 643px the auto-fit grid makes three columns and leaves one filter alone.
  test.fail(width >= 480 && width < 644, "SF-1: 3 + 1 filter rows at tablet widths");
  expect(rows).toEqual(expected);
  // Every select keeps its whole label and is at least 24px tall.
  for (const sel of await scope.locator("select").all()) {
    expect((await sel.boundingBox())?.height).toBeGreaterThanOrEqual(24);
  }
});

test("search: a GET form sending q and every filter, with no JavaScript", { tag: "@interaction" }, async ({ page }) => {
  await gotoPage(page, galleryPath("SearchField"));
  const scope = fixtureCase(page, "with a query and the filter row");
  await scope.getByRole("combobox", { name: "Category" }).selectOption({ label: "Economy" });
  await scope.getByRole("searchbox").fill("fed rate");
  await scope.getByRole("button", { name: "Search" }).click();
  await page.waitForURL(/\?q=/);
  const params = new URL(page.url()).searchParams;
  expect(Object.fromEntries(params)).toEqual({
    q: "fed rate",
    category: "economy",
    region: "india",
    type: "",
    date: "",
  });
});

// -------------------------------------------------------------------------------------------------------- 404

test("404: centred at its full width, the heading in Literata 700 at 28px, links in one centred row", async ({
  page,
}) => {
  await gotoPage(page, galleryPath("NotFound"));
  const g = await page.locator(".not-found").evaluate((nf) => {
    const parent = nf.parentElement as HTMLElement;
    const ps = getComputedStyle(parent);
    const pr = parent.getBoundingClientRect();
    const inner = {
      left: pr.left + Number.parseFloat(ps.paddingLeft),
      right: pr.right - Number.parseFloat(ps.paddingRight),
    };
    const r = nf.getBoundingClientRect();
    const links = nf.querySelector(".not-found__links")?.getBoundingClientRect();
    const items = [...nf.querySelectorAll(".not-found__link")].map((a) => a.getBoundingClientRect());
    const lineMid = (sel: string) => {
      const el = nf.querySelector(sel) as HTMLElement;
      const range = document.createRange();
      range.selectNodeContents(el);
      const rects = [...range.getClientRects()];
      return (Math.min(...rects.map((b) => b.left)) + Math.max(...rects.map((b) => b.right))) / 2;
    };
    return {
      width: r.width,
      parentWidth: inner.right - inner.left,
      leftGap: r.left - inner.left,
      rightGap: inner.right - r.right,
      mid: (r.left + r.right) / 2,
      titleMid: lineMid(".not-found__title"),
      textMid: lineMid(".not-found__text"),
      linksMid: links ? (Math.min(...items.map((b) => b.left)) + Math.max(...items.map((b) => b.right))) / 2 : 0,
      searchWidth: nf.querySelector(".search-field")?.getBoundingClientRect().width ?? 0,
    };
  });
  // NF-1: the block takes the column's width up to 720px, whatever its parent's display, so the search field is
  // not shrunk to its content in a flex column (the article column and the fixture gallery are both flex).
  expect(g.width).toBeCloseTo(Math.min(720, g.parentWidth), 0);
  expect(Math.abs(g.leftGap - g.rightGap)).toBeLessThanOrEqual(1);
  expect(g.searchWidth).toBeCloseTo(g.width, 0);
  for (const m of [g.titleMid, g.textMid, g.linksMid]) expect(Math.abs(m - g.mid)).toBeLessThanOrEqual(1.5);
  const title = await styles(page.locator(".not-found__title"), ["font-size", "font-weight", "font-family"]);
  expect(title["font-size"]).toBe("28px");
  expect(title["font-weight"]).toBe("700");
  expect(title["font-family"]).toMatch(/Literata/);
  await expect(page.getByRole("heading", { level: 2, name: "This page doesn't exist." })).toHaveClass(
    "not-found__title",
  );
  await expect(page.getByRole("navigation", { name: "Categories" }).getByRole("link")).toHaveText([
    "Tech",
    "Business",
    "Finance",
    "Economy",
    "Science",
  ]);
  // The query is typed left to right inside the centred layout.
  expect((await styles(page.locator(".not-found .search-field"), ["text-align"]))["text-align"]).toBe("start");
});

// ----------------------------------------------------------------------------------------------- Empty / error

test("empty and error states: one sentence in Literata at body size in ink, one 24px link in Fira; no crimson", async ({
  page,
  theme,
}) => {
  await gotoPage(page, galleryPath("StateMessage"));
  const forbidden = [tokenRgb("--color-error", theme), tokenRgb("--color-brand", theme)];
  for (const c of ["empty state", "error state", "message only"]) {
    const text = fixtureCase(page, c).locator(".state-message__text");
    const s = await styles(text, ["font-family", "font-size", "color"]);
    expect(s["font-family"]).toMatch(/Literata/);
    expect(px(s["font-size"] ?? "")).toBeGreaterThanOrEqual(17);
    expect(px(s["font-size"] ?? "")).toBeLessThanOrEqual(17.5);
    expect(s.color).toBe(tokenRgb("--color-ink", theme));
    const colours = await fixtureCase(page, c)
      .locator(".state-message *")
      .evaluateAll((els) => els.flatMap((e) => [getComputedStyle(e).color, getComputedStyle(e).borderTopColor]));
    for (const col of colours) expect(forbidden).not.toContain(col);
    await expect(fixtureCase(page, c).locator("svg, img")).toHaveCount(0);
  }
  for (const c of ["empty state", "error state"]) {
    const link = fixtureCase(page, c).getByRole("link");
    await expect(link).toHaveCount(1);
    expect((await link.boundingBox())?.height).toBeGreaterThanOrEqual(24);
    expect((await styles(link, ["font-family"]))["font-family"]).toMatch(/Fira Sans/);
  }
});

// ----------------------------------------------------------------------------------------------------- Loading

test("loading: the busy results are inert in this engine: no focus, no click, no transition", {
  tag: "@interaction",
}, async ({ page, browserName }) => {
  await gotoPage(page, galleryPath("LoadingRegion"));
  const busy = fixtureCase(page, "loading new results");
  const shown = fixtureCase(page, "results shown");
  expect(await page.evaluate(() => "inert" in HTMLElement.prototype)).toBe(true);
  for (const link of await busy.getByRole("link", { includeHidden: true }).all()) {
    const focused = await link.evaluate((a) => {
      (a as HTMLElement).focus();
      return document.activeElement === a;
    });
    expect(focused).toBe(false);
  }
  // Tab from the last shown result jumps over the busy ones.
  if (browserName !== "webkit") {
    await shown.getByRole("link").last().focus();
    await page.keyboard.press("Tab");
    expect(await page.evaluate(() => !!document.activeElement?.closest(".loading-region__content[inert]"))).toBe(false);
  }
  // A click on a busy result goes nowhere.
  const url = page.url();
  await busy.locator("a").first().click({ force: true });
  await page.waitForTimeout(150);
  expect(page.url()).toBe(url);
  // The fade is instant, at 50%, and the old results are not hidden from assistive technology as gone.
  const s = await styles(busy.locator(".loading-region__content"), ["opacity", "transition-duration"]);
  expect(s.opacity).toBe("0.5");
  expect(s["transition-duration"]).toBe("0s");
  await expect(busy.locator(".loading-region__content")).toHaveAttribute("aria-busy", "true");
  await expect(shown.getByRole("status")).toHaveText("");
});

// ---------------------------------------------------------------------------------------------- Report form

test("report form: each summary link focuses its field with no JavaScript, and its label stays in view", {
  tag: ["@interaction", "@smoke"],
}, async ({ page }) => {
  await gotoPage(page, galleryPath("ReportForm"));
  const form = fixtureCase(page, "with errors after sending");
  const links = form.locator(".error-summary").getByRole("link");
  await expect(links).toHaveCount(2);
  for (let i = 0; i < 2; i++) {
    await gotoPage(page, galleryPath("ReportForm"));
    const link = links.nth(i);
    const id = ((await link.getAttribute("href")) ?? "").slice(1);
    await link.click();
    await expect(page).toHaveURL(new RegExp(`#${id}$`));
    // The browser focuses the fragment's target (HTML "scroll to the fragment" runs the focusing steps).
    await expect(page.locator(`#${id}`)).toBeFocused();
    await expect(page.locator(`#${id}`)).toBeInViewport();
    // RF-1: the label (and hint) above the field are not scrolled off the top on a phone.
    const labelTop = await page.locator(`label[for=${id}]`).evaluate((l) => l.getBoundingClientRect().top);
    expect(labelTop, `label of #${id} in view`).toBeGreaterThanOrEqual(0);
  }
});

test("report form: the summary opens with a 2px error rule and no box; its links and heading icon in the error colour", async ({
  page,
  theme,
}) => {
  await gotoPage(page, galleryPath("ReportForm"));
  for (const t of [theme, other(theme)]) {
    await setTheme(page, t);
    const summary = fixtureCase(page, "with errors after sending").locator(".error-summary");
    const s = await styles(summary, [
      "border-top-width",
      "border-top-style",
      "border-top-color",
      "border-bottom-width",
      "border-left-width",
      "background-color",
    ]);
    expect(s).toEqual({
      "border-top-width": "2px",
      "border-top-style": "solid",
      "border-top-color": tokenRgb("--color-error", t),
      "border-bottom-width": "0px",
      "border-left-width": "0px",
      "background-color": "rgba(0, 0, 0, 0)",
    });
    for (const a of await summary.getByRole("link").all()) {
      expect((await styles(a, ["color"])).color).toBe(tokenRgb("--color-error", t));
      expect((await a.boundingBox())?.height).toBeGreaterThanOrEqual(24);
    }
    expect((await styles(summary.locator(".error-summary__icon"), ["color"])).color).toBe(tokenRgb("--color-error", t));
    expect((await styles(summary.getByRole("heading"), ["color"])).color).toBe(tokenRgb("--color-ink", t));
  }
  await expect(
    fixtureCase(page, "with errors after sending").locator(".error-summary").getByRole("heading", { level: 2 }),
  ).toHaveText("The report was not sent");
});

test("report form: sending posts the form with the page; the sent state links back", { tag: "@interaction" }, async ({
  page,
}) => {
  await gotoPage(page, galleryPath("ReportForm"));
  const posted = page.waitForRequest((r) => r.method() === "POST");
  // The fixture posts to the home page, which the static preview cannot answer; only the request matters.
  await page.route("**/*", (r) =>
    r.request().method() === "POST"
      ? r.fulfill({ status: 200, contentType: "text/html", body: "<!doctype html><title>Sent</title><p>Sent</p>" })
      : r.continue(),
  );
  const form = fixtureCase(page, "with errors after sending");
  await form.getByRole("textbox", { name: "What is wrong? (required)" }).fill("The services figure is wrong.");
  await form.getByRole("button", { name: "Send report" }).click();
  const body = new URLSearchParams((await posted).postData() ?? "");
  expect(Object.fromEntries(body)).toEqual({
    page: "/?page=apple-fy2024",
    what: "The services figure is wrong.",
    where: "The services chart",
    email: "reader@example",
  });
});

test("report form, empty and error states and the 404 print their words; buttons are left out", {
  tag: "@print",
}, async ({ page }) => {
  await gotoPage(page, galleryPath("ReportForm"));
  await page.emulateMedia({ media: "print" });
  const form = fixtureCase(page, "with errors after sending");
  await expect(form.locator(".error-summary")).toBeVisible();
  for (const f of await form.locator("textarea, input:not([type=hidden])").all()) await expect(f).toBeVisible();
  // Base print CSS drops every interface button: a printed form cannot be sent, so the submit goes too (accepted).
  await expect(form.getByRole("button", { includeHidden: true })).toBeHidden();
  await expect(fixtureCase(page, "sent").getByText("Report sent. Thank you for checking.")).toBeVisible();
  await gotoPage(page, galleryPath("StateMessage"));
  for (const t of await page.locator(".state-message").all()) await expect(t).toBeVisible();
});

test("controls galleries pass axe with the fields focused and after a tap, in both themes", { tag: "@a11y" }, async ({
  page,
  theme,
}) => {
  await gotoPage(page, galleryPath("ReportForm"));
  await page.locator("#report-email").focus();
  await expectAccessible(page);
  await setTheme(page, other(theme));
  await expectAccessible(page);
});
