// Component tester's checks for #11 part 2 (controls and states), on top of the builder's controls.test.tsx: every
// prop and default of each control; the field shell's wiring in every combination; the report form's checks on edge
// input and the summary's order; content edge cases; axe on the cases the gallery does not show (the blank form);
// the index budget; and server rendering with hydration for the parts islands will render (LoadingRegion, fields).
import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Button } from "../../src/components/Button/Button";
import { Checkbox } from "../../src/components/Checkbox/Checkbox";
import { LoadingRegion } from "../../src/components/LoadingRegion/LoadingRegion";
import { buildComponentIndex, componentSpecs, indexBudgetProblems } from "../../src/components/library";
import { SECTIONS } from "../../src/components/Masthead/Masthead";
import { NotFound } from "../../src/components/NotFound/NotFound";
import { ErrorSummary, REPORT_FIELD_IDS, ReportForm, reportErrors } from "../../src/components/ReportForm/ReportForm";
import { specs } from "../../src/components/registry.generated";
import { SearchField } from "../../src/components/SearchField/SearchField";
import { Select } from "../../src/components/Select/Select";
import { StateMessage } from "../../src/components/StateMessage/StateMessage";
import { TextArea } from "../../src/components/TextArea/TextArea";
import { TextField } from "../../src/components/TextField/TextField";
import { hydrateAndCollectWarnings } from "../support/hydrate";
import { expectNoAxeViolations, renderComponent } from "../support/render";

const CONTROLS = [
  "Button",
  "TextField",
  "TextArea",
  "Select",
  "Checkbox",
  "SearchField",
  "StateMessage",
  "NotFound",
  "LoadingRegion",
  "ReportForm",
];

describe("Button", () => {
  it("defaults: a secondary type=button, so it never submits a form by accident", () => {
    renderComponent(<Button>Show the data table</Button>);
    const b = screen.getByRole("button", { name: "Show the data table" });
    expect(b.getAttribute("type")).toBe("button");
    expect(b.className).toBe("button button--secondary");
  });

  it("every variant, a submit, extra classes and native attributes pass through", () => {
    renderComponent(
      <form>
        <Button variant="primary" type="submit" className="x y" name="go" value="1">
          Send report
        </Button>
        <Button variant="text" aria-describedby="r" disabled>
          Download CSV
        </Button>
        <span id="r">Unavailable for this licence.</span>
      </form>,
    );
    const primary = screen.getByRole("button", { name: "Send report" });
    expect(primary.className).toBe("button button--primary x y");
    expect([primary.getAttribute("type"), primary.getAttribute("name"), primary.getAttribute("value")]).toEqual([
      "submit",
      "go",
      "1",
    ]);
    const text = screen.getByRole("button", { name: "Download CSV" });
    expect(text.className).toBe("button button--text");
    expect(text).toBeDisabled();
    expect(text).toHaveAccessibleDescription("Unavailable for this licence.");
  });
});

describe("field shell (TextField/field.tsx), through every control", () => {
  it("name falls back to the id; an explicit name wins", () => {
    renderComponent(
      <>
        <TextField id="a" label="A" />
        <TextField id="b" name="where" label="B" />
        <TextArea id="c" label="C" />
        <Select id="d" label="D" options={[{ value: "", label: "Any" }]} />
        <Checkbox id="e" name="primary" label="E" />
      </>,
    );
    const names = ["a", "b", "c", "d", "e"].map((id) => document.getElementById(id)?.getAttribute("name"));
    expect(names).toEqual(["a", "where", "c", "d", "primary"]);
  });

  it("describedby holds only what exists: hint, error, both or nothing", () => {
    renderComponent(
      <>
        <TextField id="h" label="H" hint="Hint." />
        <TextField id="e" label="E" error="Wrong." />
        <TextArea id="he" label="HE" hint="Hint." error="Wrong." />
        <Select id="n" label="N" options={[]} />
      </>,
    );
    const by = (id: string) => document.getElementById(id)?.getAttribute("aria-describedby") ?? null;
    expect([by("h"), by("e"), by("he"), by("n")]).toEqual(["h-hint", "e-error", "he-hint he-error", null]);
    for (const id of ["h-hint", "e-error", "he-hint", "he-error"]) expect(document.getElementById(id)).not.toBeNull();
    expect(screen.getByRole("textbox", { name: "HE" })).toHaveAccessibleDescription(/^Hint. Error: ?Wrong.$/); // jsdom drops the hidden span's trailing space; browsers keep it
  });

  it("the error says 'Error:' to screen readers, its icon is hidden, and the wrapper takes the error class", () => {
    const { container } = renderComponent(<TextField id="x" label="X" error="Enter a page address." />);
    const error = container.querySelector(".field__error") as HTMLElement;
    expect(error.textContent).toBe("Error: Enter a page address.");
    expect(error.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
    expect(container.querySelector(".field")?.classList.contains("field--error")).toBe(true);
  });

  it("required=false is optional, not 'not required': no attribute, '(optional)' in the label", () => {
    renderComponent(<Select id="s" label="Region" required={false} options={[{ value: "", label: "All" }]} />);
    const s = screen.getByRole("combobox", { name: "Region (optional)" });
    expect(s.hasAttribute("required")).toBe(false);
    expect(s.hasAttribute("aria-invalid")).toBe(false);
  });

  it("the marker is part of the label element, so it is part of the accessible name", () => {
    renderComponent(<Checkbox id="k" label="I checked the source" required />);
    const k = screen.getByRole("checkbox", { name: "I checked the source (required)" });
    expect(k.hasAttribute("required")).toBe(true);
  });
});

describe("TextField and TextArea", () => {
  it("types: email, url and tel turn spell-checking off; text leaves it to the browser", () => {
    renderComponent(
      <>
        {(["text", "email", "url", "tel"] as const).map((t) => (
          <TextField key={t} id={t} type={t} label={t} autoComplete={t === "email" ? "email" : undefined} />
        ))}
      </>,
    );
    const spell = (id: string) => document.getElementById(id)?.getAttribute("spellcheck") ?? null;
    expect([spell("text"), spell("email"), spell("url"), spell("tel")]).toEqual([null, "false", "false", "false"]);
    expect(document.getElementById("email")?.getAttribute("autocomplete")).toBe("email");
    expect(document.getElementById("email")?.getAttribute("type")).toBe("email");
  });

  it("values come back as the reader typed them, line breaks, markup and very long words included", () => {
    const long = `https://example.com/${"a".repeat(400)}`;
    renderComponent(
      <>
        <TextField id="t" label="T" value={long} />
        <TextArea id="a" label="A" value={"Line one\n<b>not bold</b> & more\nLine three"} rows={2} />
      </>,
    );
    expect((document.getElementById("t") as HTMLInputElement).value).toBe(long);
    const a = document.getElementById("a") as HTMLTextAreaElement;
    expect(a.value).toBe("Line one\n<b>not bold</b> & more\nLine three");
    expect(a.getAttribute("rows")).toBe("2");
    expect(a.querySelector("b")).toBeNull();
  });

  it("a text area has five rows by default", () => {
    renderComponent(<TextArea id="a" label="A" />);
    expect(screen.getByRole("textbox", { name: "A" }).getAttribute("rows")).toBe("5");
  });
});

describe("Select and Checkbox", () => {
  it("select: options in order, the first chosen by default, an unknown value falls back to the first", () => {
    const options = [
      { value: "", label: "All regions" },
      { value: "india", label: "India" },
      { value: "europe", label: "Europe" },
    ];
    renderComponent(
      <>
        <Select id="a" label="A" options={options} />
        <Select id="b" label="B" options={options} value="atlantis" />
      </>,
    );
    const a = screen.getByRole("combobox", { name: "A" }) as HTMLSelectElement;
    expect(
      within(a)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["All regions", "India", "Europe"]);
    expect(a.value).toBe("");
    expect((screen.getByRole("combobox", { name: "B" }) as HTMLSelectElement).value).toBe("");
  });

  it("checkbox: unchecked by default, sends its value, hint and error under the label", () => {
    const { container } = renderComponent(
      <Checkbox
        id="k"
        name="primary"
        value="yes"
        label="Primary sources only"
        hint="Filings and releases."
        error="No."
      />,
    );
    const k = screen.getByRole("checkbox", { name: "Primary sources only" }) as HTMLInputElement;
    expect(k.checked).toBe(false);
    expect(k.value).toBe("yes");
    expect(k).toHaveAccessibleDescription(/^Filings and releases. Error: ?No.$/);
    const text = container.querySelector(".checkbox__text") as HTMLElement;
    expect([...text.children].map((c) => c.className)).toEqual(["field__label", "field__hint", "field__error"]);
  });
});

describe("SearchField", () => {
  it("defaults: /search/, id search-q, 'Search pages', no filter row, a primary submit", () => {
    const { container } = renderComponent(<SearchField />);
    const form = screen.getByRole("search", { name: "Search pages" });
    expect(form.getAttribute("action")).toBe("/search/");
    expect(container.querySelector("#search-q")).not.toBeNull();
    expect(container.querySelector(".search-field__filters")).toBeNull();
    const submit = within(form).getByRole("button", { name: "Search" });
    expect([submit.getAttribute("type"), submit.className]).toEqual([
      "submit",
      "button button--primary search-field__submit",
    ]);
    expect((within(form).getByRole("searchbox") as HTMLInputElement).value).toBe("");
  });

  it("a query with quotes and markup is a value, never markup; filters go inside the same form", () => {
    renderComponent(
      <SearchField
        id="q2"
        label="Search pages, second"
        query={`"apple" <b>services</b> & co`}
        filters={<Select id="r" name="region" label="Region" options={[{ value: "", label: "All" }]} />}
      />,
    );
    const form = screen.getByRole("search", { name: "Search pages, second" });
    expect((within(form).getByRole("searchbox", { name: "Search pages, second" }) as HTMLInputElement).value).toBe(
      `"apple" <b>services</b> & co`,
    );
    expect(within(form).getByRole("combobox", { name: "Region" })).toBeTruthy();
  });
});

describe("StateMessage, NotFound, LoadingRegion", () => {
  it("state message: the sentence, and a link only when there is an action", () => {
    const { container, rerender } = renderComponent(<StateMessage message="No pages in this section yet." />);
    expect(container.querySelectorAll("a")).toHaveLength(0);
    expect(container.textContent).toBe("No pages in this section yet.");
    rerender(<StateMessage message="No pages match these filters." action={{ href: "/s/?q=a", label: "Clear" }} />);
    expect(screen.getByRole("link", { name: "Clear" }).getAttribute("href")).toBe("/s/?q=a");
  });

  it("404 by default: one h1, the five categories from the masthead, its own search and nav names", () => {
    renderComponent(<NotFound />);
    expect(screen.getAllByRole("heading", { level: 1 }).map((h) => h.textContent)).toEqual([
      "This page doesn't exist.",
    ]);
    expect(screen.getByText("It may have moved, or the link may be wrong.")).toBeTruthy();
    const links = within(screen.getByRole("navigation", { name: "Categories" })).getAllByRole("link");
    expect(links.map((l) => [l.textContent, l.getAttribute("href")])).toEqual(SECTIONS.map((s) => [s.label, s.href]));
    const search = screen.getByRole("search", { name: "Search pages" });
    expect(search.getAttribute("action")).toBe("/search/");
    expect(within(search).getByRole("searchbox").id).toBe("not-found-q");
  });

  it("loading: a custom status line; idle content is neither busy nor inert", () => {
    const { container, rerender } = renderComponent(
      <LoadingRegion busy loadingText="Loading pages">
        <a href="/a">A</a>
      </LoadingRegion>,
    );
    expect(screen.getByRole("status").textContent).toBe("Loading pages");
    rerender(
      <LoadingRegion busy={false} loadingText="Loading pages">
        <a href="/a">A</a>
      </LoadingRegion>,
    );
    const content = container.querySelector(".loading-region__content") as HTMLElement;
    expect(content.hasAttribute("inert")).toBe(false);
    expect(content.hasAttribute("aria-busy")).toBe(false);
    expect(screen.getByRole("status").textContent).toBe("");
    // The status element is there from the first render, so the change is announced.
    expect(container.querySelector("[role=status]")?.parentElement).toBe(container.firstElementChild);
  });
});

describe("report-an-error form: checks", () => {
  it("nothing given: only 'what' is wrong", () => {
    expect(reportErrors({})).toEqual({ what: "Say what is wrong." });
  });

  it("'what' with tabs, newlines and non-breaking spaces only is empty", () => {
    expect(reportErrors({ what: "\t\n  " }).what).toBe("Say what is wrong.");
  });

  it("email: shapes it accepts and refuses; spaces around are trimmed, spaces inside are not", () => {
    for (const ok of ["a@b.co", "first.last+news@sub.example.org", "  reader@example.com\n", "ünïcode@exämple.de"]) {
      expect(reportErrors({ what: "x", email: ok }), ok).toEqual({});
    }
    for (const bad of ["reader", "reader@", "@example.com", "reader@example", "read er@example.com", "a@b@c.de"]) {
      expect(reportErrors({ what: "x", email: bad }).email, bad).toBe(
        "Enter an email address like name@example.com, or leave it empty.",
      );
    }
    expect(reportErrors({ what: "x", email: "   " })).toEqual({});
  });

  it("'where' is never required and a very long report is the intake's business, not the form's", () => {
    expect(reportErrors({ what: "w".repeat(20_000), where: "" })).toEqual({});
  });
});

describe("report-an-error form: rendering", () => {
  it("the blank form passes axe and has no summary content", async () => {
    const { container } = renderComponent(<ReportForm action="/report/" pageUrl="/p/" />);
    await expectNoAxeViolations(container);
    expect(container.querySelector(".error-summary")?.getAttribute("aria-live")).toBe("polite");
    expect(container.querySelectorAll("[aria-invalid]")).toHaveLength(0);
    expect(container.querySelector("form")?.hasAttribute("novalidate")).toBe(true);
    expect(screen.getAllByRole("button").map((b) => b.textContent)).toEqual(["Send report"]);
  });

  it("an intake error on 'where' sits between the other two in the summary, in field order", async () => {
    const errors = { email: "E.", where: "W.", what: "Q." };
    const { container } = renderComponent(<ReportForm action="/r/" pageUrl="/p/" errors={errors} />);
    const links = within(container.querySelector(".error-summary") as HTMLElement).getAllByRole("link");
    expect(links.map((l) => [l.textContent, l.getAttribute("href")])).toEqual([
      ["Q.", `#${REPORT_FIELD_IDS.what}`],
      ["W.", `#${REPORT_FIELD_IDS.where}`],
      ["E.", `#${REPORT_FIELD_IDS.email}`],
    ]);
    // Each link's target exists and is the control itself, so following it focuses the field.
    for (const l of links) {
      const target = container.querySelector(l.getAttribute("href") ?? "");
      expect(target?.matches("input, textarea")).toBe(true);
    }
    await expectNoAxeViolations(container);
  });

  it("errors for unknown keys are ignored rather than linking nowhere", () => {
    const { container } = renderComponent(
      <ReportForm action="/r/" pageUrl="/p/" errors={{ bogus: "Nope." } as never} />,
    );
    expect(container.querySelector(".error-summary")?.childElementCount).toBe(0);
  });

  it("the page address travels exactly, query string included; the sent link goes back to it", () => {
    const url = "/finance/apple-fy2024/?ref=x&y=1#chart";
    const { container, rerender } = renderComponent(<ReportForm action="/r/" pageUrl={url} />);
    expect(container.querySelector<HTMLInputElement>("input[name=page]")?.value).toBe(url);
    rerender(<ReportForm action="/r/" pageUrl={url} sent />);
    expect(screen.getByRole("link", { name: "Back to the page" }).getAttribute("href")).toBe(url);
    expect(container.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  });

  it("ErrorSummary on its own: a heading with the title and a list of links; empty, an empty live region", () => {
    const { container, rerender } = renderComponent(
      <ErrorSummary title="The search was not run" errors={[{ id: "q", message: "Enter some words." }]} />,
    );
    expect(screen.getByRole("heading", { level: 2, name: "The search was not run" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "Enter some words." }).getAttribute("href")).toBe("#q");
    rerender(<ErrorSummary title="The search was not run" errors={[]} />);
    expect(container.querySelector(".error-summary")?.innerHTML).toBe("");
  });
});

describe("index (ADR 0001)", () => {
  it("the controls are primitives: none is in the page writers' index, which stays within budget", () => {
    const index = buildComponentIndex(componentSpecs(specs));
    expect(indexBudgetProblems(index)).toEqual([]);
    const names = index.components.map((c) => c.name);
    for (const n of CONTROLS) expect(names).not.toContain(n);
    for (const n of CONTROLS) {
      expect(specs.find((s) => s.name === n)?.kind, n).toBe("primitive");
    }
  });
});

describe("server rendering", () => {
  it("controls render complete, script-free HTML and hydrate with no warnings (busy and idle)", async () => {
    for (const busy of [true, false]) {
      const ui = (
        <>
          <SearchField
            query="apple"
            filters={<Select id="r" label="Region" options={[{ value: "", label: "All" }]} />}
          />
          <LoadingRegion busy={busy}>
            <a href="/a">A result</a>
          </LoadingRegion>
          <TextField id="t" label="T" hint="H" error="E" required value="v" />
          <TextArea id="a" label="A" value={"x\ny"} />
          <Checkbox id="c" label="C" checked />
          <Button variant="primary">Go</Button>
          <StateMessage message="Nothing." action={{ href: "/", label: "Home" }} />
        </>
      );
      const { html, warnings } = await hydrateAndCollectWarnings(ui);
      expect(warnings).toEqual([]);
      expect(html).not.toMatch(/<script|data-island|\son[a-z]+="/i);
      expect(html.includes('inert=""'), `busy=${busy}`).toBe(busy);
    }
  });
});
