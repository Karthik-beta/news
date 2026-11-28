// A chart's social image (design/charts.md, "Social images"): a 1200×675 light-mode PNG of one chart as the site
// draws it, with the page URL at the bottom left and the wordmark at the bottom right, rendered on the laptop by
// Playwright's Chromium (never in the Worker). The page is laid out at 720×405 CSS px and captured at 5/3 device
// pixels, so the smallest chart text (12px) is 20px in the image. A chart too tall for that card gets a 1200×1200
// square (720×720 CSS px); one too tall for the square is an error, never shrunk.
//   bun scripts/social-image.ts <page path> <chart id> [--base http://127.0.0.1:4321] [--out chart.png]
// The site must be running at --base (`bun run dev`, or the built Worker). The chart id is its statement name.
import { writeFileSync } from "node:fs";
import { basename } from "node:path";
import type { Browser, Page } from "@playwright/test";

export const SOCIAL = { width: 1200, height: 675, squareHeight: 1200, minTextPx: 20 } as const;
/** The site's origin (astro.config.mjs `site`), for pages without a canonical link (fixture pages). Not imported from
 * src/config/site.ts, which imports JSON that Playwright's loader cannot. */
const ORIGIN = "https://news.karthik.nz";
const SCALE = 5 / 3;
const VIEWPORT = { width: SOCIAL.width / SCALE, height: SOCIAL.height / SCALE };
/** The card's heights in image px, tried in order: the 16:9 card, then the square for a chart too tall for it. */
const HEIGHTS = [SOCIAL.height, SOCIAL.squareHeight] as const;

/** The chart is taller than the card, even with its plot given up to the smallest height allowed. */
export class TooTallError extends Error {}
/** 48px of padding in the image, in CSS px. */
const PAD = `${48 / SCALE}px`;

const CARD_CSS = `
:root { color-scheme: light; }
body > :not(.social-card) { display: none !important; }
.social-card {
  position: fixed; inset: 0; box-sizing: border-box; padding: ${PAD};
  display: flex; flex-direction: column; gap: var(--space-4); overflow: hidden; background: var(--color-paper);
}
.social-card figure { flex: 1; min-height: 0; margin: 0; }
.social-card .figure-frame__actions, .social-card .choropleth__table, .social-card .choropleth__bars { display: none; }
.social-card__foot { display: flex; justify-content: space-between; align-items: baseline; margin: 0; }
.social-card__url { font-family: var(--font-family-sans); font-size: var(--font-size-source); color: var(--color-ink-3); }
.social-card__wordmark { color: var(--color-ink-3); }
`;

/** Lays the page out as the social card for one chart, at the page's viewport. Throws when the chart is missing or
 * has no data, and `TooTallError` when it does not fit. */
export async function layoutSocialCard(page: Page, chart: string, url: string): Promise<void> {
  await page.evaluate(() => document.fonts.ready.then(() => undefined));
  await page.addStyleTag({ content: CARD_CSS });
  const problem = await page.evaluate(
    ({ chart, url }) => {
      document.documentElement.dataset.theme = "light";
      const figure = document.getElementById(`${chart}-title`)?.closest("figure");
      if (!figure) return `no chart "${chart}" on the page`;
      const card = document.createElement("div");
      card.className = "social-card";
      const foot = document.createElement("p");
      foot.className = "social-card__foot";
      const link = document.createElement("span");
      link.className = "social-card__url";
      link.textContent = url;
      const mark = document.createElement("span");
      mark.className = "social-card__wordmark text-wordmark";
      mark.textContent = "News";
      foot.append(link, mark);
      card.append(figure.cloneNode(true), foot);
      document.body.append(card);
      // The original is hidden, and a pattern inside a hidden tree draws nothing: the copy points at its own.
      for (const pattern of card.querySelectorAll("pattern[id]")) {
        const old = pattern.id;
        pattern.id = `${old}-social`;
        for (const el of card.querySelectorAll(`[fill="url(#${old})"]`)) el.setAttribute("fill", `url(#${pattern.id})`);
      }
      const fig = card.querySelector("figure") as HTMLElement;
      if (fig.querySelector(".chart-figure__missing")) return `chart "${chart}" has no data to show`;
      // The plot gives up height for the card (never the text its size): a chart's plot box, or the map.
      const over = fig.scrollHeight - fig.clientHeight;
      const plots = [...card.querySelectorAll<HTMLElement>(".chart-plot__inner, .choropleth__map")];
      if (over > 1 && plots.length > 0) {
        // Plots side by side (small multiples) shrink together, so the height is shared between rows, not plots.
        const rows = new Set(plots.map((p) => Math.round(p.getBoundingClientRect().top))).size;
        const share = over / rows;
        for (const p of plots) {
          const h = Math.max(96, p.getBoundingClientRect().height - share);
          if (p.matches(".choropleth__map")) Object.assign(p.style, { height: `${h}px`, width: "auto" });
          else p.closest<HTMLElement>(".chart-plot")?.style.setProperty("--chart-plot-height", `${h}px`);
        }
      }
      return fig.scrollHeight > fig.clientHeight + 1
        ? `too tall: chart "${chart}" is ${fig.scrollHeight}px tall; the card has room for ${fig.clientHeight}px`
        : null;
    },
    { chart, url },
  );
  if (problem?.startsWith("too tall: ")) throw new TooTallError(problem.slice("too tall: ".length));
  if (problem) throw new Error(problem);
}

/** The smallest text in the card, in image pixels (CSS font size × device scale × any SVG scaling). */
export async function smallestTextPx(page: Page): Promise<{ px: number; text: string }> {
  return page.evaluate((scale) => {
    let smallest = { px: Number.POSITIVE_INFINITY, text: "" };
    const card = document.querySelector(".social-card") as Element;
    for (const el of card.querySelectorAll("*")) {
      const own = [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent?.trim());
      const box = el.getBoundingClientRect();
      if (!own || box.width === 0 || box.height === 0 || getComputedStyle(el).visibility === "hidden") continue;
      const svgScale = el instanceof SVGGraphicsElement ? Math.abs(el.getScreenCTM()?.a ?? 1) : 1;
      const px = Number.parseFloat(getComputedStyle(el).fontSize) * svgScale * scale;
      if (px < smallest.px) smallest = { px, text: el.textContent?.trim() ?? "" };
    }
    return smallest;
  }, SCALE);
}

/** Opens the page in a fresh light-mode context sized for the card and returns the PNG: 1200×675, or the 1200×1200
 * square when the chart is too tall for that. */
export async function socialImage(
  browser: Browser,
  { base, path, chart }: { base: string; path: string; chart: string },
): Promise<{ png: Buffer; page: Page }> {
  // A path is a path on `base`: "//other.example/x" or "https://other.example/" would open another site.
  const target = new URL(path, base);
  if (!path.startsWith("/") || target.origin !== new URL(base).origin) {
    throw new Error(`page path must start with "/" and stay on ${base}: ${path}`);
  }
  const context = await browser.newContext({
    viewport: VIEWPORT,
    deviceScaleFactor: SCALE,
    colorScheme: "light",
    reducedMotion: "reduce",
  });
  const page = await context.newPage();
  for (const height of HEIGHTS) {
    const viewport = { width: VIEWPORT.width, height: height / SCALE };
    await page.setViewportSize(viewport);
    await page.goto(target.href); // a fresh load for each size: the layout rewrites the page
    const canonical = await page.evaluate(() => document.querySelector("link[rel='canonical']")?.getAttribute("href"));
    const url = new URL(canonical ?? new URL(path, ORIGIN).href);
    try {
      await layoutSocialCard(page, chart, `${url.host}${url.pathname}`);
    } catch (error) {
      if (error instanceof TooTallError && height !== HEIGHTS.at(-1)) continue;
      throw error;
    }
    const png = await page.screenshot({ clip: { x: 0, y: 0, ...viewport }, animations: "disabled", caret: "hide" });
    return { png, page };
  }
  throw new Error("unreachable: the last size returns or throws");
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  const flag = (name: string, fallback: string) => {
    const i = args.indexOf(name);
    return i >= 0 ? (args.splice(i, 2)[1] ?? fallback) : fallback;
  };
  const base = flag("--base", "http://127.0.0.1:4321");
  const outFlag = flag("--out", "");
  const [path, chart] = args;
  const out = outFlag || `${basename(chart ?? "")}.png`;
  if (!path || !chart) {
    console.error("usage: bun scripts/social-image.ts <page path> <chart id> [--base URL] [--out file.png]");
    process.exit(2);
  }
  const { chromium } = await import("@playwright/test");
  const browser = await chromium.launch();
  try {
    const { png, page } = await socialImage(browser, { base, path, chart });
    const smallest = await smallestTextPx(page);
    if (smallest.px < SOCIAL.minTextPx) throw new Error(`text under 20px: "${smallest.text}" at ${smallest.px}px`);
    writeFileSync(out, png);
    // PNG header: width and height are the big-endian words at bytes 16 and 20.
    console.log(`${out}: ${png.readUInt32BE(16)}×${png.readUInt32BE(20)}, smallest text ${smallest.px.toFixed(1)}px`);
  } finally {
    await browser.close();
  }
}
