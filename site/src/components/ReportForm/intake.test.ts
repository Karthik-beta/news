// The report form's mapping to the intake: field names, the page ID, the limits. The body is checked against the
// intake's own schema (the precompiled validator the api Worker uses), so the two cannot drift apart.
import { schemas } from "@news/worker-shared/validate";
import { describe, expect, it } from "vitest";
import { backPath, intakeBody, LIMITS, limitErrors, pageIdOf } from "./intake";

const ORIGIN = "https://news.karthik.nz";
const m = { textTooShort: "short", textTooLong: "long", whereTooLong: "where long" };

describe("intakeBody", () => {
  it("maps what to text, trims, drops empty optional fields, and is valid for the intake", () => {
    const body = intakeBody({ what: "  The table shows 3.1, the source says 3.2.  ", where: " ", email: "" }, "us-cpi");
    expect(body).toEqual({ page_id: "us-cpi", text: "The table shows 3.1, the source says 3.2." });
    expect(schemas.validateReportIntake(body)).toBe(true);
  });

  it("carries where, email and the honeypot when they are filled", () => {
    const body = intakeBody({ what: "x".repeat(10), where: "Chart title", email: "a@b.co" }, "p", "http://spam");
    expect(body).toEqual({
      page_id: "p",
      text: "xxxxxxxxxx",
      where: "Chart title",
      email: "a@b.co",
      website: "http://spam",
    });
    expect(schemas.validateReportIntake(body)).toBe(true);
  });
});

describe("pageIdOf", () => {
  it.each([
    ["us-cpi-december-2024", "us-cpi-december-2024"],
    ["/2025/01/us-cpi-december-2024", "us-cpi-december-2024"],
    ["/2025/01/us-cpi-december-2024/", "us-cpi-december-2024"],
    [`${ORIGIN}/2025/01/us-cpi-december-2024/index.md`, "us-cpi-december-2024"],
    [`  ${ORIGIN}/2025/01/us-cpi-december-2024#chart `, "us-cpi-december-2024"],
  ])("%s is %s", (input, id) => {
    expect(pageIdOf(input, ORIGIN)).toBe(id);
  });

  it.each([
    "",
    "Not An ID",
    "https://example.org/2025/01/us-cpi",
    "/economy/inflation",
    "/2025/01",
    "/2025/01/a/b",
    "javascript:alert(1)",
  ])("%j is no page", (input) => {
    expect(pageIdOf(input, ORIGIN)).toBeUndefined();
  });

  it("links back to a pasted address, never to an ID or another site", () => {
    expect(backPath("/2025/01/us-cpi", ORIGIN)).toBe("/2025/01/us-cpi");
    expect(backPath("us-cpi", ORIGIN)).toBe("/");
    expect(backPath("https://example.org/2025/01/us-cpi", ORIGIN)).toBe("/");
  });
});

describe("limitErrors", () => {
  it("uses the contract's limits in characters and the API's limits in bytes", () => {
    expect(limitErrors({ what: "x".repeat(LIMITS.textMin - 1) }, m).what).toBe("short");
    expect(limitErrors({ what: "x".repeat(LIMITS.textMin) }, m)).toEqual({});
    expect(limitErrors({ what: "x".repeat(LIMITS.textMax + 1) }, m).what).toBe("long");
    // 1,500 characters of 3 bytes each: under the character limit, over the 4,000 byte cap.
    expect(limitErrors({ what: "€".repeat(1500) }, m).what).toBe("long");
    expect(limitErrors({ what: "x".repeat(20), where: "y".repeat(LIMITS.whereMax + 1) }, m).where).toBe("where long");
    expect(limitErrors({ what: "x".repeat(20), where: "😀".repeat(160) }, m).where).toBe("where long");
  });

  it("leaves an empty report to the form's own check", () => {
    expect(limitErrors({ what: "  " }, m)).toEqual({});
  });
});
