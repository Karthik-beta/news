// The report form on its own page (/report): it asks for the page's address and the kind of error, where the form
// reached from an article carries the page in a hidden field. The kinds are the intake contract's.
import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import report from "../../../contracts/cloud/report.schema.json";
import { REPORT_KINDS, ReportForm } from "../../src/components/ReportForm/ReportForm";
import { renderComponent } from "../support/render";

describe("ReportForm standalone", () => {
  it("asks for the page address and the kind, with no hidden page field", () => {
    const { container } = renderComponent(<ReportForm action="/x" pageUrl="/" standalone />);
    expect(screen.getByRole("textbox", { name: "Address of the page (required)" }).getAttribute("name")).toBe("page");
    expect(screen.getByRole("combobox", { name: "What kind of error? (optional)" }).getAttribute("name")).toBe("kind");
    expect(container.querySelector('input[type="hidden"][name="page"]')).toBeNull();
    expect(container.querySelectorAll('[name="page"]')).toHaveLength(1);
  });

  it("is unchanged by default: the page travels in a hidden field", () => {
    const { container } = renderComponent(<ReportForm action="/x" pageUrl="/p/" />);
    expect(container.querySelector('input[type="hidden"][name="page"]')?.getAttribute("value")).toBe("/p/");
    expect(screen.queryByRole("combobox")).toBeNull();
  });

  it("offers exactly the kinds the intake accepts, and a first option that sends none", () => {
    expect(REPORT_KINDS.map((k) => k.value).filter(Boolean)).toEqual(report.$defs.kind.enum);
    expect(REPORT_KINDS[0].value).toBe("");
  });
});
