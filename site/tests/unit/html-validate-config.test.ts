import { join } from "node:path";
import { FileSystemConfigLoader, HtmlValidate } from "html-validate";
import { describe, expect, it } from "vitest";
import { siteRoot } from "../support/tokens";

// The integration project validates every page in dist/ with site/.htmlvalidate.json. These tests pin what that
// config accepts (Astro's minified output) and what it must still reject, so loosening a rule shows up here.
const validator = new HtmlValidate(new FileSystemConfigLoader());
const filename = join(siteRoot, "dist", "example.html"); // resolves site/.htmlvalidate.json like a built page

const doc = (body: string, head = "") =>
  `<!DOCTYPE html><html lang="en-GB"><head><meta charset="utf-8"><title>T</title>${head}</head><body>${body}</body></html>`;

async function ruleIds(html: string): Promise<string[]> {
  const report = await validator.validateString(html, filename);
  return report.results.flatMap((r) => r.messages.map((m) => m.ruleId)).sort();
}

describe("HTML validation rules (.htmlvalidate.json)", () => {
  it("accepts a page in the shape Astro builds (whitespace collapsed, attributes quoted)", async () => {
    const html = doc(
      '<main class="dev"><p class="fly">Economy</p><h1>Prices rose 2.4%</h1><p>Text.</p><img src="/a.png" alt="A chart"></main>',
      '<meta name="viewport" content="width=device-width, initial-scale=1"><link rel="stylesheet" href="/_astro/a.css">',
    );
    expect(await ruleIds(html)).toEqual([]);
  });

  it.each([
    ["duplicate id", '<p id="a">x</p><p id="a">y</p>', "no-dup-id"],
    ["image without alt", '<img src="/a.png">', "wcag/h37"],
    ["button without a type", "<button>Go</button>", "no-implicit-button-type"],
    ["element in the wrong parent", "<ul><p>x</p></ul>", "element-permitted-content"],
    ["unclosed element", "<main><div>x</main>", "close-order"],
    ["deprecated element", "<center>x</center>", "deprecated"],
  ])("rejects a %s", async (_what, body, rule) => {
    expect(await ruleIds(doc(body))).toContain(rule);
  });

  it("accepts the camelCase attribute names React writes on the server; still rejects upper case", async () => {
    // React 19 renders autoComplete, spellCheck and noValidate as written; HTML attribute names are case-insensitive.
    const form =
      '<form action="/r/" method="post" noValidate><label for="e">E</label><input id="e" name="e" type="email" autoComplete="email" spellCheck="false"><button type="submit">Send</button></form>';
    expect(await ruleIds(doc(form))).toEqual([]);
    expect(await ruleIds(doc('<p CLASS="a">x</p>'))).toContain("attr-case");
  });

  it("rejects a page without lang or title", async () => {
    const ids = await ruleIds('<!DOCTYPE html><html><head><meta charset="utf-8"></head><body></body></html>');
    expect(ids).toEqual(["element-required-attributes", "element-required-content"]);
  });

  it("requires integrity on cross-origin scripts only", async () => {
    expect(await ruleIds(doc('<script src="/_astro/a.js"></script>'))).not.toContain("require-sri");
    expect(await ruleIds(doc('<script src="https://cdn.example.org/a.js"></script>'))).toContain("require-sri");
  });
});
