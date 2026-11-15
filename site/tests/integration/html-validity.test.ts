import { FileSystemConfigLoader, formatterFactory, HtmlValidate } from "html-validate";
import { describe, expect, it } from "vitest";
import { htmlFiles, urlPathOf } from "../support/dist";

// Rules: site/.htmlvalidate.json (html-validate:recommended, plus the document rules).
const validator = new HtmlValidate(new FileSystemConfigLoader());
const text = formatterFactory("text");

describe("HTML validity of the built site", () => {
  const files = htmlFiles();

  it("built at least one page", () => {
    expect(files.length).toBeGreaterThan(0);
  });

  it.each(files.map((f) => [urlPathOf(f), f]))("%s is valid HTML", async (_path, file) => {
    const report = await validator.validateFile(file);
    expect(report.valid, text(report.results)).toBe(true);
  });
});
