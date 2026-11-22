// Builder's sanity checks for the #11 controls and states: fields wire their label, hint and error to the control;
// required and optional say so in the label; the report form's error summary links to each field and its own
// checks match the design's wording; the loading state marks the old results busy and inert. Every fixture case
// also passes axe through primitives.test.tsx. The formal tests are the component tester's.
import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Checkbox } from "../../src/components/Checkbox/Checkbox";
import { LoadingRegion } from "../../src/components/LoadingRegion/LoadingRegion";
import { REPORT_FIELD_IDS, ReportForm, reportErrors } from "../../src/components/ReportForm/ReportForm";
import { SearchField } from "../../src/components/SearchField/SearchField";
import { Select } from "../../src/components/Select/Select";
import { TextField } from "../../src/components/TextField/TextField";
import { renderComponent } from "../support/render";

describe("form fields", () => {
  it("label, hint and error describe the control; the error marks it invalid", () => {
    renderComponent(<TextField id="f" label="Page address" hint="From the address bar." error="Enter the address." />);
    const input = screen.getByRole("textbox", { name: "Page address" });
    expect(input.getAttribute("aria-describedby")).toBe("f-hint f-error");
    expect(input.getAttribute("aria-invalid")).toBe("true");
    expect(screen.getByText("Enter the address.").closest("p")?.textContent).toBe("Error: Enter the address.");
  });

  it("required, optional and no marker", () => {
    renderComponent(
      <>
        <TextField id="a" label="A" required />
        <TextField id="b" label="B" required={false} />
        <TextField id="c" label="C" />
      </>,
    );
    const [a, b, c] = ["A (required)", "B (optional)", "C"].map((name) => screen.getByRole("textbox", { name }));
    expect(a?.hasAttribute("required")).toBe(true);
    expect(b?.hasAttribute("required")).toBe(false);
    expect(c?.hasAttribute("aria-describedby")).toBe(false);
    expect(c?.hasAttribute("aria-invalid")).toBe(false);
  });

  it("select and checkbox keep the value they were rendered with", () => {
    renderComponent(
      <>
        <Select
          id="s"
          name="region"
          label="Region"
          options={[
            { value: "", label: "All regions" },
            { value: "india", label: "India" },
          ]}
          value="india"
        />
        <Checkbox id="k" label="Primary sources only" checked />
      </>,
    );
    expect((screen.getByRole("combobox", { name: "Region" }) as HTMLSelectElement).value).toBe("india");
    expect((screen.getByRole("checkbox", { name: "Primary sources only" }) as HTMLInputElement).checked).toBe(true);
  });

  it("the search field is a GET search landmark sending q", () => {
    const { container } = renderComponent(<SearchField query="apple" />);
    const form = screen.getByRole("search", { name: "Search pages" });
    expect(form.getAttribute("method")).toBe("get");
    expect(form.getAttribute("action")).toBe("/search/");
    expect((within(form).getByRole("searchbox", { name: "Search pages" }) as HTMLInputElement).value).toBe("apple");
    expect(container.querySelector("input[name=q]")).not.toBeNull();
  });
});

describe("report-an-error form", () => {
  it("checks: what is required, email optional but well formed", () => {
    expect(reportErrors({ what: "The chart is wrong" })).toEqual({});
    expect(reportErrors({ what: "x", email: "" })).toEqual({});
    expect(reportErrors({ what: " \n " })).toEqual({ what: "Say what is wrong." });
    expect(reportErrors({ what: "x", email: "reader@example" }).email).toMatch(/^Enter an email address/);
    expect(reportErrors({ what: "x", email: " reader@example.com " })).toEqual({});
  });

  it("the error summary links to each field with an error, in field order", () => {
    const values = { what: "", where: "The services chart", email: "nope" };
    const { container } = renderComponent(
      <ReportForm action="/report/" pageUrl="/finance/apple/" values={values} errors={reportErrors(values)} />,
    );
    const links = within(container.querySelector(".error-summary") as HTMLElement).getAllByRole("link");
    expect(links.map((l) => l.getAttribute("href"))).toEqual([
      `#${REPORT_FIELD_IDS.what}`,
      `#${REPORT_FIELD_IDS.email}`,
    ]);
    for (const l of links) {
      const field = container.querySelector(l.getAttribute("href") ?? "");
      expect(field?.getAttribute("aria-invalid")).toBe("true");
      expect(field?.getAttribute("aria-describedby")).toContain(`${field?.id}-error`);
    }
    // The reader's words come back; the page travels as a hidden field.
    expect((screen.getByRole("textbox", { name: "Where on the page? (optional)" }) as HTMLInputElement).value).toBe(
      "The services chart",
    );
    expect(container.querySelector<HTMLInputElement>("input[type=hidden][name=page]")?.value).toBe("/finance/apple/");
    const form = container.querySelector("form");
    expect([form?.getAttribute("method"), form?.getAttribute("action")]).toEqual(["post", "/report/"]);
    expect(screen.getByRole("button", { name: "Send report" }).getAttribute("type")).toBe("submit");
  });

  it("with no errors the summary is empty; the labels match the design", () => {
    const { container } = renderComponent(<ReportForm action="/report/" pageUrl="/p/" />);
    expect(container.querySelector(".error-summary")?.childElementCount).toBe(0);
    for (const name of ["What is wrong? (required)", "Where on the page? (optional)"]) {
      expect(screen.getByRole("textbox", { name })).toBeTruthy();
    }
    expect(screen.getByRole("textbox", { name: "Your email, if you'd like a reply (optional)" })).toBeTruthy();
  });

  it("sent: the confirmation and a link back to the page, no form", () => {
    const { container } = renderComponent(<ReportForm action="/report/" pageUrl="/p/" sent />);
    expect(container.querySelector("form")).toBeNull();
    expect(screen.getByRole("status").textContent).toContain("Report sent. Thank you for checking.");
    expect(screen.getByRole("link", { name: "Back to the page" }).getAttribute("href")).toBe("/p/");
  });

  it("an off-site or protocol-relative page falls back to / in the link and the hidden field", () => {
    for (const pageUrl of ["https://evil.example", "//evil.example"]) {
      const sent = renderComponent(<ReportForm action="/report/" pageUrl={pageUrl} sent />);
      expect(screen.getByRole("link", { name: "Back to the page" }).getAttribute("href")).toBe("/");
      sent.unmount();
      const { container } = renderComponent(<ReportForm action="/report/" pageUrl={pageUrl} />);
      expect(container.querySelector<HTMLInputElement>('input[name="page"]')?.value).toBe("/");
    }
  });
});

describe("loading state", () => {
  it("busy: the old results stay, marked busy and inert, and the status says so", () => {
    const { container, rerender } = renderComponent(
      <LoadingRegion busy={false}>
        <p>Result</p>
      </LoadingRegion>,
    );
    const content = container.querySelector(".loading-region__content");
    expect(content?.hasAttribute("aria-busy")).toBe(false);
    expect(screen.getByRole("status").textContent).toBe("");
    rerender(
      <LoadingRegion busy>
        <p>Result</p>
      </LoadingRegion>,
    );
    expect(content?.getAttribute("aria-busy")).toBe("true");
    expect(content?.hasAttribute("inert")).toBe(true);
    expect(screen.getByText("Result")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toBe("Loading results");
  });
});
