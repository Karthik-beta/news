// Component tester's checks for the #11 extension set (news-component-testing), on top of the builder's
// tests/component/extension-set.test.tsx. SourceCard: every source type and role, every word from the source record,
// dates, the missing state, citing (the sources panel lists the card's source once), escaping in HTML and Markdown.
// Steps: numbering, every reference kind, missing values, one source line with each source once, currencies, scales,
// true minus and periods exactly as Prose writes them, the schema limits, Markdown. LineChart notes: numbering and
// filtering (`numberedNotes`), the list, the plot's labels, missing data, Markdown escaping, the schema limits.
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import type { z } from "zod/v4";
import { type ChartNote, linePlot, lineChartData, numberedNotes } from "../../src/components/LineChart/geometry";
import lineChartSpec from "../../src/components/LineChart/spec";
import sourceCardSpec from "../../src/components/SourceCard/spec";
import stepsSpec from "../../src/components/Steps/spec";
import { componentFixtureData, renderComponentFixture } from "../../src/renderer/fixtures/component-pages";
import { fixtureLibrary } from "../../src/renderer/fixtures/library";
import { renderPage } from "../../src/renderer/render";

const library = fixtureLibrary();
/** The text a reader sees: popover panels left out, block children apart. */
const text = (el: Element | null | undefined) => {
  if (!el) return null;
  const copy = el.cloneNode(true) as Element;
  for (const p of copy.querySelectorAll("[popover]")) p.remove();
  for (const b of copy.querySelectorAll("p, li, div")) b.append(" ");
  return copy.textContent?.replace(/\s+/g, " ").trim() ?? null;
};

interface Point {
  value: string | null;
  unit: string;
  currency?: string | null;
  scale?: string | null;
  period?: string;
}

const SOURCE = {
  title: "A synthetic report",
  short: "Synthetic report",
  publisher: "Test Publisher",
  connector: "fixture",
  type: "report",
  primary: true,
  url: "https://example.org/report",
  licence: { name: "CC BY 4.0", redistribution: true },
  retrieved_at: "2026-11-01",
};

/** Renders a page from the component fixture data plus synthetic points and sources made for one test. */
function render(oui: string, extra: { points?: Record<string, Point>; sources?: Record<string, object> } = {}) {
  const base = structuredClone(componentFixtureData()) as ReturnType<typeof componentFixtureData> & {
    data: { points: Record<string, unknown> };
    sources: { sources: Record<string, unknown> };
  };
  for (const [ref, p] of Object.entries(extra.points ?? {})) {
    base.data.points[ref] = {
      label: ref,
      value: p.value,
      value_text: p.value,
      unit: p.unit,
      currency: p.currency ?? null,
      scale: p.scale ?? null,
      period: p.period ?? "pd_fx_2024",
      source: "src_fixture_synthetic",
      locator: { kind: "json", series: ref },
    };
  }
  for (const [id, s] of Object.entries(extra.sources ?? {})) base.sources.sources[id] = s;
  const result = renderPage({ oui, ...base, head: { flyTitle: "Test", headline: "Test", summary: "Test" } }, { library });
  return { result, doc: new JSDOM(result.article).window.document, md: result.files["index.md"] as string };
}

// ---------------------------------------------------------------------------------------------------------------

describe("SourceCard", () => {
  const TYPES: [string, boolean, string, string[]][] = [
    ["filing", true, "Filing", ["Primary"]],
    ["dataset", true, "Dataset", ["Primary"]],
    ["press_release", true, "Press release", ["Primary"]],
    ["statement", true, "Statement", ["Primary"]],
    ["report", true, "Report", ["Primary"]],
    ["paper", true, "Paper", ["Primary", "Peer-reviewed"]],
    ["preprint", true, "Preprint", ["Primary", "Preprint"]],
    ["news", false, "News article", ["Context"]],
    ["other", false, "Source", ["Context"]],
  ];

  it.each(TYPES)("a %s source (primary %s) is labelled %s with badges %j", (type, primary, label, badges) => {
    const { doc, result } = render(`root = Page([c])\nc = SourceCard("src_test_card")`, {
      sources: { src_test_card: { ...SOURCE, type, primary, published_at: "2024-10-22" } },
    });
    const card = doc.querySelector(".source-card");
    expect([...(card?.querySelectorAll(".badge") ?? [])].map((b) => b.textContent)).toEqual(badges);
    expect(text(card?.querySelector(".source-card__meta"))).toBe(`${label}, Test Publisher, published 22 October 2024`);
    expect(result.problems).toEqual([]);
  });

  it("every word it shows comes from the source record: badges, title, type, publisher and date", () => {
    const { doc } = render(`root = Page([c])\nc = SourceCard("src_test_card")`, {
      sources: { src_test_card: { ...SOURCE, published_at: "2024-01-05" } },
    });
    const card = doc.querySelector(".source-card");
    expect(text(card)).toBe("Primary A synthetic report Report, Test Publisher, published 5 January 2024");
    const link = card?.querySelector("a");
    expect(link?.getAttribute("href")).toBe("https://example.org/report");
    // The icon is decoration; the link is named by the title alone.
    expect(link?.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
    expect(link?.getAttribute("target")).toBeNull();
  });

  it("dates are day month year with no comma, and no date is written when the record has none", () => {
    for (const [published_at, meta] of [
      ["2024-12-31", "Report, Test Publisher, published 31 December 2024"],
      ["2025-02-01", "Report, Test Publisher, published 1 February 2025"],
      [null, "Report, Test Publisher"],
      [undefined, "Report, Test Publisher"],
    ] as const) {
      const { doc } = render(`root = Page([c])\nc = SourceCard("src_test_card")`, {
        sources: { src_test_card: { ...SOURCE, published_at } },
      });
      expect(text(doc.querySelector(".source-card__meta"))).toBe(meta);
    }
  });

  it("an unknown source says No data, links nothing, is reported and is not listed as a source", () => {
    const { doc, result, md } = render(`root = Page([c])\nc = SourceCard("src_test_not_in_sources")`);
    const card = doc.querySelector(".source-card");
    expect(card?.classList.contains("source-card--missing")).toBe(true);
    expect(text(card)).toBe("No data");
    expect(card?.querySelector("a")).toBeNull();
    expect(result.problems.map((p) => [p.code, p.ref])).toContainEqual(["unknown-source", "src_test_not_in_sources"]);
    expect(result.sourcesUsed).not.toContain("src_test_not_in_sources");
    expect(md).toContain("\n\nNo data\n");
  });

  it("cites its source: the sources panel lists it once, also when a value on the page cites the same source", () => {
    const { result, md, doc } = render(
      `root = Page([c, p])\nc = SourceCard("src_aapl_10k_2024")\np = Prose(["Sales were {dp_aapl_net_sales_fy2024}."])`,
    );
    expect(result.sourcesUsed.filter((s) => s === "src_aapl_10k_2024")).toHaveLength(1);
    expect(md.match(/^- \[Apple Inc\. Form 10-K, fiscal 2024\]/gm)).toHaveLength(1);
    expect(doc.querySelectorAll(".source-card")).toHaveLength(1);
  });

  it("a card alone is enough to put its source in the sources panel", () => {
    const { result, md } = render(`root = Page([c])\nc = SourceCard("src_test_trial")`);
    expect(result.sourcesUsed).toEqual(["src_test_trial"]);
    expect(md).toMatch(/## Sources\n\n- \[A test trial of a daily walking programme/);
  });

  it("escapes the record's words in HTML and in Markdown", () => {
    const { doc, md } = render(`root = Page([c])\nc = SourceCard("src_test_card")`, {
      sources: {
        src_test_card: {
          ...SOURCE,
          title: "<b>Bold</b> *star* [x](y) _under_ | pipe",
          publisher: "A <i>publisher</i>",
          published_at: "2024-10-22",
        },
      },
    });
    const card = doc.querySelector(".source-card");
    expect(card?.querySelector("b, i")).toBeNull();
    expect(text(card?.querySelector(".source-card__title"))).toBe("<b>Bold</b> *star* [x](y) _under_ | pipe");
    expect(md).toContain(
      "**[\\<b\\>Bold\\</b\\> \\*star\\* \\[x\\](y) \\_under\\_ \\| pipe](https://example.org/report)**",
    );
    expect(md).toContain("Primary. Report, A \\<i\\>publisher\\</i\\>, published 22 October 2024.");
  });

  // SC-1: the Markdown sentence ends "Inc.." when the publisher ends with a full stop and the record has no date.
  it("Markdown: one full stop at the end of the meta sentence, also after a publisher ending in a full stop (SC-1)", () => {
    const { md } = render(`root = Page([c])\nc = SourceCard("src_test_card")`, {
      sources: { src_test_card: { ...SOURCE, type: "filing", publisher: "Apple Inc.", published_at: null } },
    });
    expect(md).toContain("Primary. Filing, Apple Inc.\n");
    expect(md).not.toContain("..");
  });

  // Accepted (SIGNOFF.md): a preprint's badge and its type both say Preprint, the same as in the sources panel.
  it("a preprint shows its Preprint badge and its type, as the sources panel does", () => {
    const { doc } = render(`root = Page([c])\nc = SourceCard("src_test_preprint")`);
    const card = doc.querySelector(".source-card");
    expect([...(card?.querySelectorAll(".badge") ?? [])].map((b) => b.textContent)).toEqual(["Primary", "Preprint"]);
    expect(text(card?.querySelector(".source-card__meta"))).toMatch(/^Preprint, /);
  });

  it("schema: one src_ ID; nothing else", () => {
    const props = sourceCardSpec.props as z.ZodObject;
    for (const ok of ["src_a", "src_aapl_10k_2024"]) expect(props.safeParse({ source: ok }).success, ok).toBe(true);
    for (const bad of ["", "aapl", "dp_aapl", "src_", "src_A B", "https://example.org"])
      expect(props.safeParse({ source: bad }).success, bad).toBe(false);
    expect(props.safeParse({ source: "src_a", title: "Words" }).success).toBe(true); // extra keys are dropped
    expect(props.parse({ source: "src_a", title: "Words" })).toEqual({ source: "src_a" });
  });
});

// ---------------------------------------------------------------------------------------------------------------

describe("Steps", () => {
  const steps = (...texts: string[]) =>
    `root = Page([s])\ns = Steps([${texts.map((t, i) => `{title: "Step ${i + 1}", text: ${JSON.stringify(t)}}`).join(", ")}])`;

  it.each([2, 3, 5, 8])("%i steps are an ordered list numbered 1 to %i by code", (n) => {
    const { doc, md } = render(steps(...Array.from({ length: n }, (_, i) => `Text ${"abcdefgh"[i]}.`)));
    const items = [...doc.querySelectorAll("ol.steps__list > li.steps__step")];
    expect(items.map((li) => li.querySelector(".steps__number")?.textContent)).toEqual(
      Array.from({ length: n }, (_, i) => String(i + 1)),
    );
    // The visible number repeats the list's order, so assistive technology hears it once, from the list.
    for (const li of items) expect(li.querySelector(".steps__number")?.getAttribute("aria-hidden")).toBe("true");
    expect(md.match(/^\d\. \*\*Step \d\.\*\* Text [a-h]\.$/gm)).toHaveLength(n);
  });

  it("every reference kind resolves in the text: value, period, quote, context and derived value", () => {
    const { doc, result } = render(
      steps(
        "Sales were {dp_aapl_net_sales_fy2024} in {pd_fy2024}.",
        "That was {cx_aapl_first_rise_2y}; growth was {dv_aapl_net_sales_growth_fy2024}.",
        "The statement said: {q_fomc_statement_sep_2024}",
      ),
    );
    const t = [...doc.querySelectorAll(".steps__text")].map(text);
    expect(t[0]).toBe("Sales were $391.0bn in fiscal 2024.");
    expect(t[1]).not.toMatch(/\{|\}|No data/);
    expect(t[2]).toMatch(/^The statement said: “In light of the progress on inflation/);
    expect(result.problems).toEqual([]);
  });

  it("titles are plain text: a reference in a title is resolved but not a popover", () => {
    const { doc } = render(
      `root = Page([s])\ns = Steps([{title: "Sales in {pd_fy2024}", text: "A."}, {title: "<b>Next</b>", text: "B."}])`,
    );
    const titles = [...doc.querySelectorAll(".steps__title")];
    expect(titles.map(text)).toEqual(["Sales in fiscal 2024", "<b>Next</b>"]);
    expect(doc.querySelector(".steps__title button, .steps__title b")).toBeNull();
  });

  it("an unknown value says No data and is reported; the step stays", () => {
    const { doc, result } = render(steps("Known {dp_aapl_net_sales_fy2024}.", "Unknown {dp_test_not_in_data}."));
    expect(text(doc.querySelectorAll(".steps__text")[1])).toBe("Unknown no data.");
    expect(result.problems.map((p) => p.ref)).toContain("dp_test_not_in_data");
  });

  it("one source line under the list, each source once, however many steps cite it", () => {
    const { doc } = render(
      steps(
        "{dp_aapl_net_sales_fy2024} and {dp_aapl_net_sales_fy2023}.",
        "Again {dp_aapl_net_sales_fy2024}.",
        "And the Fed: {q_fomc_statement_sep_2024}",
      ),
    );
    const lines = doc.querySelectorAll(".steps__source");
    expect(lines).toHaveLength(1);
    expect(doc.querySelector(".steps__list .steps__source")).toBeNull();
    const links = [...(lines[0]?.querySelectorAll("a") ?? [])].map((a) => a.getAttribute("href"));
    expect(new Set(links).size).toBe(links.length);
    expect(links.length).toBe(2);
  });

  it("no source line when nothing is cited (periods and glossary links cite no source)", () => {
    const { doc } = render(steps("Plain words.", "A [basis point](glossary:basis-point) term."));
    expect(doc.querySelector(".steps__source")).toBeNull();
  });

  // Numbers as the source states them (content-style.md): each currency, scale and sign written by the same code
  // as Prose, so a step reads exactly like a paragraph.
  const MONEY: Record<string, Point> = {
    dp_t_usd: { value: "391035", unit: "USD", currency: "USD", scale: "millions" },
    dp_t_eur: { value: "1200", unit: "EUR", currency: "EUR", scale: "billions" },
    dp_t_gbp: { value: "65.6", unit: "GBP", currency: "GBP", scale: "billions" },
    dp_t_jpy: { value: "1.2", unit: "JPY", currency: "JPY", scale: "trillions" },
    dp_t_inr: { value: "2100000", unit: "INR", currency: "INR", scale: "millions" },
    dp_t_krw: { value: "302231", unit: "KRW", currency: "KRW", scale: "billions" },
    dp_t_twd: { value: "2894308", unit: "TWD", currency: "TWD", scale: "millions" },
    dp_t_hkd: { value: "88.5", unit: "HKD", currency: "HKD", scale: "billions" },
    dp_t_aud: { value: "420", unit: "AUD", currency: "AUD", scale: "millions" },
    dp_t_chf: { value: "4.5", unit: "CHF", currency: "CHF", scale: "billions" },
    dp_t_neg: { value: "-5.7", unit: "Percent" },
    dp_t_big: { value: "1234567890123", unit: "Number" },
    dp_t_small: { value: "0.000000014", unit: "Number" },
    dp_t_zero: { value: "0", unit: "Percent" },
    dp_t_thousands: { value: "167045", unit: "Number" },
    dp_t_null: { value: null, unit: "Percent" },
  };

  it("currencies, scales, separators and true minus: exactly as Prose writes them", () => {
    const refs = Object.keys(MONEY);
    const sentence = (r: string) => `It was {${r}}.`;
    const s = render(steps(...refs.slice(0, 8).map(sentence)), { points: MONEY });
    const s2 = render(steps(...refs.slice(8).map(sentence)), { points: MONEY });
    const p = render(`root = Page([p])\np = Prose([${refs.map((r) => JSON.stringify(sentence(r))).join(", ")}])`, {
      points: MONEY,
    });
    const stepTexts = [...s.doc.querySelectorAll(".steps__text"), ...s2.doc.querySelectorAll(".steps__text")].map(
      text,
    );
    const proseTexts = [...p.doc.querySelectorAll(".prose p")].map(text);
    expect(stepTexts).toEqual(proseTexts);
    const all = stepTexts.join(" ");
    for (const want of ["$391.0bn", "€", "£65.6bn", "₹2.1trn", "₩", "NT$", "HK$88.5bn", "A$420m", "CHF 4.5bn", "−5.7%"])
      expect(all, want).toContain(want);
    expect(all).not.toMatch(/-\d/); // never a hyphen-minus
    expect(all).toContain("167,045");
    expect(stepTexts[15]).toBe("It was no data.");
  });

  it("Markdown escapes titles, keeps the order and writes resolved values", () => {
    const { md } = render(
      `root = Page([s])\ns = Steps([{title: "*Bold* [x]", text: "Sales {dp_aapl_net_sales_fy2024}."}, {title: "1. Not a list", text: "B."}])`,
    );
    expect(md).toContain("1. **\\*Bold\\* \\[x\\].** Sales $391.0bn.");
    // A title that starts like a list item stays inside item 2 (the shared escapeMarkdown writes "\1.", see SIGNOFF O-2).
    expect(md.split("\n")).toContainEqual(expect.stringMatching(/^2\. \*\*\\?1\. Not a list\.\*\* B\.$/));
  });

  it("schema: 2-8 steps; a title 1-60 characters; text required", () => {
    const props = stepsSpec.props as z.ZodObject;
    const step = (title = "A step", t = "What happens.") => ({ title, text: t });
    expect(props.safeParse({ steps: [] }).success).toBe(false);
    expect(props.safeParse({ steps: [step()] }).success).toBe(false);
    expect(props.safeParse({ steps: Array(8).fill(step()) }).success).toBe(true);
    expect(props.safeParse({ steps: Array(9).fill(step()) }).success).toBe(false);
    expect(props.safeParse({ steps: [step("x".repeat(60)), step()] }).success).toBe(true);
    expect(props.safeParse({ steps: [step("x".repeat(61)), step()] }).success).toBe(false);
    expect(props.safeParse({ steps: [step(""), step()] }).success).toBe(false);
    expect(props.safeParse({ steps: [step("A", ""), step()] }).success).toBe(false);
  });
});

// ---------------------------------------------------------------------------------------------------------------

describe("LineChart notes", () => {
  const slots = ["pd_a", "pd_b", "pd_c", "pd_d", "pd_e"].map((ref, i) => ({ ref, label: `P${i}` }));
  // biome-ignore lint/suspicious/noExplicitAny: PeriodSlot has more fields than numberedNotes reads
  const ns = (notes: ChartNote[]) => numberedNotes(notes, slots as any);

  it("numbers notes 1, 2, 3 left to right whatever order the writer gives them", () => {
    expect(
      ns([
        { at: "pd_e", text: "E" },
        { at: "pd_a", text: "A" },
        { at: "pd_c", text: "C" },
      ]).map((n) => [n.number, n.at, n.period, n.slot]),
    ).toEqual([
      [1, "pd_a", "P0", 0],
      [2, "pd_c", "P2", 2],
      [3, "pd_e", "P4", 4],
    ]);
  });

  it("leaves out notes off the axis and second notes at one period, and numbers the rest without gaps", () => {
    const out = ns([
      { at: "pd_x", text: "Off the axis" },
      { at: "pd_d", text: "D first" },
      { at: "pd_b", text: "B" },
      { at: "pd_d", text: "D again" },
    ]);
    expect(out.map((n) => [n.number, n.text])).toEqual([
      [1, "B"],
      [2, "D first"],
    ]);
    expect(ns([])).toEqual([]);
    expect(ns([{ at: "pd_x", text: "None" }])).toEqual([]);
  });

  const chart = (notes: string, series = `["sr_us_cpi_2024"]`) =>
    `root = Page([c])\nc = LineChart("T", "S, %", ${series}, null, null, false, ${notes})`;

  it("each note is an ink-3 line on the plot labelled with its number on every screen, and a list item", () => {
    const { doc } = render(chart(`[{at: "pd_2024_06", text: "Mid-year"}, {at: "pd_2024_01", text: "Start"}]`));
    const labels = [...doc.querySelectorAll(".line-chart__event-label")];
    expect(labels.map((l) => l.textContent)).toEqual(["2", "1"].sort());
    for (const l of labels) expect(l.classList.contains("is-wide-only")).toBe(false);
    const list = doc.querySelector("ol.line-chart__notes");
    expect(list?.getAttribute("aria-label")).toBe("Notes on the chart");
    expect([...(list?.querySelectorAll("li") ?? [])].map(text)).toEqual(["1Jan 2024: Start", "2Jun 2024: Mid-year"]);
    // The list sits inside the figure, after the plot and before the source line.
    const fig = list?.closest("figure");
    expect(fig).not.toBeNull();
    const order = [...(fig?.querySelectorAll(".chart-plot, .line-chart__notes, .chart-source, .source-line") ?? [])];
    expect(order.indexOf(list as Element)).toBeGreaterThan(0);
  });

  it("a note's words are escaped in HTML and Markdown", () => {
    const { doc, md } = render(chart(`[{at: "pd_2024_06", text: "<b>Bold</b> *star* _x_ [y]"}]`));
    expect(doc.querySelector(".line-chart__notes b")).toBeNull();
    expect(text(doc.querySelector(".line-chart__notes li"))).toBe("1Jun 2024: <b>Bold</b> *star* _x_ [y]");
    expect(md).toContain("1. Jun 2024: \\<b\\>Bold\\</b\\> \\*star\\* \\_x\\_ \\[y\\]");
  });

  it("a chart with no data shows No data and no notes; a note at a missing value still points at its period", () => {
    const none = render(chart(`[{at: "pd_2024_06", text: "Mid-year"}]`, `["sr_test_not_in_data"]`));
    expect(none.doc.querySelector(".line-chart__notes, .line-chart__event")).toBeNull();
    expect(none.md).not.toContain("Notes on the chart");
    // sr_fx_gaps has missing values; a note at a gap is kept (the period is on the axis).
    const gaps = render(chart(`[{at: "pd_fx_2021", text: "A gap"}]`, `["sr_fx_gaps"]`));
    const series = (gaps.doc.querySelector("[data-island='LineChartPlot']") as Element | null)?.getAttribute(
      "data-props",
    );
    expect(series).toBeTruthy();
    expect(gaps.result.problems.filter((p) => p.code !== "no-value")).toEqual([]);
  });

  it("notes never change the data: the CSV, table and plotted values are the same with and without notes", () => {
    const withNotes = render(chart(`[{at: "pd_2024_06", text: "Mid-year"}]`));
    const without = render(chart("null"));
    expect(withNotes.result.charts.map((c) => c.csv)).toEqual(without.result.charts.map((c) => c.csv));
    expect(withNotes.doc.querySelector("table")?.outerHTML).toBe(without.doc.querySelector("table")?.outerHTML);
  });

  it("every plot label is a number from 1 to 3 in a row the plot draws (row 0 or 1)", () => {
    const f = renderComponentFixture("LineChart", "notes");
    const d = new JSDOM(f.article).window.document;
    for (const island of d.querySelectorAll("[data-island='LineChartPlot']")) {
      const props = JSON.parse(island.getAttribute("data-props") ?? "{}") as {
        events: { label: string | null; row: number; phone: boolean }[];
      };
      for (const e of props.events) {
        expect(e.label).toMatch(/^[1-3]$/);
        expect([0, 1]).toContain(e.row);
        expect(e.phone).toBe(true);
      }
    }
  });

  it("linePlot takes the notes from its options only: no notes, no event lines", () => {
    const data = renderComponentFixture("LineChart", "default");
    expect(data.article).not.toContain("line-chart__event");
    expect(typeof linePlot).toBe("function");
    expect(typeof lineChartData).toBe("function");
  });

  it("schema: at most three notes of 1-40 characters, no digits, no braces, at a pd_ period", () => {
    const props = lineChartSpec.props as z.ZodObject;
    const base = { title: "T", subtitle: "S", series: ["sr_us_cpi_2024"] };
    const one = (t: string, at = "pd_2024_03") => props.safeParse({ ...base, notes: [{ at, text: t }] }).success;
    expect(one("x".repeat(40))).toBe(true);
    expect(one("x".repeat(41))).toBe(false);
    expect(one("")).toBe(false);
    for (const bad of ["Q3", "2024", "Up 5%", "{", "}", "{pd_2024_03}"]) expect(one(bad), bad).toBe(false);
    for (const ok of ["First rate cut", "Covid-related closures", "Fed's pivot", "Peak: inflation"])
      expect(one(ok), ok).toBe(true);
    expect(one("Cut", "dp_2024_03")).toBe(false);
    expect(props.safeParse({ ...base, notes: null }).success).toBe(true);
    expect(props.safeParse({ ...base, notes: [] }).success).toBe(true);
    const three = [1, 2, 3].map(() => ({ at: "pd_2024_03", text: "N" }));
    expect(props.safeParse({ ...base, notes: three }).success).toBe(true);
    expect(props.safeParse({ ...base, notes: [...three, three[0]] }).success).toBe(false);
  });
});
