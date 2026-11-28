// Builder's sanity checks for the #11 extension set (site/src/components/EXTENSIONS.md): SourceCard, Steps and the
// LineChart notes. Every word a SourceCard shows comes from sources.json and the source joins the sources panel;
// Steps are an ordered list numbered by code with one source line; chart notes are numbered left to right, drawn
// on the plot and listed under it, and their schema takes no digits and no {ref}s. Fixture pages also pass axe in
// both themes through chrome-text-media.test.tsx. The formal tests are the component tester's.
import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";
import { z } from "zod/v4";
import lineChartSpec from "../../src/components/LineChart/spec";
import sourceCardSpec from "../../src/components/SourceCard/spec";
import stepsSpec from "../../src/components/Steps/spec";
import { renderComponentFixture } from "../../src/renderer/fixtures/component-pages";

const doc = (component: string, fixture: string) =>
  new JSDOM(renderComponentFixture(component, fixture).article).window.document;
const figure = (d: Document, id: string) => d.getElementById(`${id}-title`)?.closest("figure");
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? null;

describe("SourceCard", () => {
  it("shows the source record's badges, title, link and meta, and adds the source to the page's sources", () => {
    const result = renderComponentFixture("SourceCard", "default");
    const d = new JSDOM(result.article).window.document;
    const cards = [...d.querySelectorAll(".source-card")];
    expect(cards.map((c) => [...c.querySelectorAll(".badge")].map((b) => b.textContent))).toEqual([
      ["Primary"],
      ["Primary", "Peer-reviewed"],
    ]);
    const link = cards[0]?.querySelector(".source-card__title a");
    expect(text(link)).toBe("Apple Inc. Form 10-K, fiscal 2024");
    expect(link?.getAttribute("href")).toBe(
      "https://www.sec.gov/Archives/edgar/data/320193/000032019324000123/aapl-20240928.htm",
    );
    expect(link?.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
    expect(text(cards[0]?.querySelector(".source-card__meta"))).toBe("Filing, Apple Inc., published 1 November 2024");
    // No publication date in the record: none is written.
    expect(text(cards[1]?.querySelector(".source-card__meta"))).toBe("Paper, News test fixtures");
    expect(result.sourcesUsed).toEqual(expect.arrayContaining(["src_aapl_10k_2024", "src_test_trial"]));
    expect(result.files["index.md"]).toContain(
      "**[Apple Inc. Form 10-K, fiscal 2024](https://www.sec.gov/Archives/edgar/data/320193/000032019324000123/aapl-20240928.htm)**",
    );
  });

  it("marks a context source as Context and says No data for an unknown source", () => {
    const result = renderComponentFixture("SourceCard", "edge-cases");
    const d = new JSDOM(result.article).window.document;
    expect([...d.querySelectorAll(".source-card .badge")].map((b) => b.textContent)).toEqual([
      "Primary",
      "Preprint",
      "Context",
    ]);
    expect(text(d.querySelector(".source-card--missing"))).toBe("No data");
    expect(result.problems.map((p) => p.code)).toContain("unknown-source");
  });

  it("takes one src_ ID and no words", () => {
    const json = z.toJSONSchema(sourceCardSpec.props as z.ZodObject) as { properties: Record<string, unknown> };
    expect(Object.keys(json.properties)).toEqual(["source"]);
    const props = sourceCardSpec.props as z.ZodObject;
    expect(props.safeParse({ source: "src_aapl_10k_2024" }).success).toBe(true);
    expect(props.safeParse({ source: "Apple's annual report" }).success).toBe(false);
  });
});

describe("Steps", () => {
  it("is an ordered list numbered by code, with one source line for everything the steps cite", () => {
    const result = renderComponentFixture("Steps", "default");
    const d = new JSDOM(result.article).window.document;
    const steps = [...d.querySelectorAll("ol.steps__list > li.steps__step")];
    expect(steps).toHaveLength(4);
    expect(steps.map((s) => s.querySelector(".steps__number")?.textContent)).toEqual(["1", "2", "3", "4"]);
    for (const s of steps) expect(s.querySelector(".steps__number")?.getAttribute("aria-hidden")).toBe("true");
    expect(d.querySelectorAll(".steps__source")).toHaveLength(1);
    expect(result.files["index.md"]).toMatch(/^1\. \*\*The committee meets\.\*\* /m);
    expect(result.files["index.md"]).toMatch(/^4\. \*\*Markets move to the new range\.\*\* /m);
  });

  it("has no source line when nothing is cited and says No data for an unknown value", () => {
    const d = doc("Steps", "edge-cases");
    const first = d.querySelector(".steps");
    expect(first?.querySelector(".steps__source")).toBeNull();
    expect(first?.querySelectorAll(".source-ref--missing").length).toBeGreaterThan(0);
    expect(d.querySelectorAll(".steps")[2]?.querySelectorAll(".steps__step")).toHaveLength(8);
  });

  it("takes two to eight steps", () => {
    const props = stepsSpec.props as z.ZodObject;
    const step = { title: "A step", text: "What happens." };
    expect(props.safeParse({ steps: [step] }).success).toBe(false);
    expect(props.safeParse({ steps: [step, step] }).success).toBe(true);
    expect(props.safeParse({ steps: Array(9).fill(step) }).success).toBe(false);
  });
});

describe("LineChart notes", () => {
  it("numbers notes left to right, draws each number on the plot and lists the period and words under it", () => {
    const result = renderComponentFixture("LineChart", "notes");
    const d = new JSDOM(result.article).window.document;
    const cpi = figure(d, "cpi");
    expect([...(cpi?.querySelectorAll(".line-chart__event-label") ?? [])].map((l) => l.textContent)).toEqual([
      "1",
      "2",
      "3",
    ]);
    // Every number shows on phones too: no note label is wide-only.
    expect(cpi?.querySelectorAll(".line-chart__event-label.is-wide-only")).toHaveLength(0);
    const notes = [...(cpi?.querySelectorAll(".line-chart__notes li") ?? [])];
    expect(notes.map((n) => n.getAttribute("data-ref"))).toEqual(["pd_2024_03", "pd_2024_09", "pd_2024_12"]);
    expect(text(notes[1])).toMatch(/^2Sep 2024: First rate cut$/);
    expect(result.files["index.md"]).toContain("Notes on the chart:");
  });

  it("leaves out a note whose period is not on the axis, and a second note at one period", () => {
    const d = doc("LineChart", "notes");
    const odd = figure(d, "odd");
    const notes = [...(odd?.querySelectorAll(".line-chart__notes li") ?? [])].map((n) => text(n));
    expect(notes).toEqual(["1fiscal 2022: Noted once"]);
    expect(odd?.querySelectorAll(".line-chart__event")).toHaveLength(1);
    // Neighbouring notes all keep a label.
    expect(figure(d, "close")?.querySelectorAll(".line-chart__event-label")).toHaveLength(3);
  });

  it("charts without notes are unchanged: no list, no event lines", () => {
    const d = doc("LineChart", "default");
    expect(d.querySelectorAll(".line-chart__notes, .line-chart__event")).toHaveLength(0);
  });

  it("a note is a few words: no digits, no {ref}s, at most three, at a pd_ period", () => {
    const props = lineChartSpec.props as z.ZodObject;
    const base = { title: "T", subtitle: "S", series: ["sr_us_cpi_2024"] };
    const note = (text: string, at = "pd_2024_03") => ({ ...base, notes: [{ at, text }] });
    expect(props.safeParse(note("First rate cut")).success).toBe(true);
    expect(props.safeParse(note("Rate hits 5%")).success).toBe(false);
    expect(props.safeParse(note("{dp_us_cpi_2024_03}")).success).toBe(false);
    expect(props.safeParse(note("Spring", "March")).success).toBe(false);
    expect(props.safeParse({ ...base, notes: Array(4).fill({ at: "pd_2024_03", text: "Too many" }) }).success).toBe(
      false,
    );
  });
});
