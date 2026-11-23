// Plate, the warm render server (src/renderer/plate.ts), in Bun: its handler with the production library, then the
// real server on a free port. `news render` and `news ai accept` call these endpoints.
import { describe, expect, test } from "bun:test";
import { fileURLToPath } from "node:url";
import { readFixtureBundles } from "../../src/renderer/fixtures/bundles/fs";
import { check, handle, type render } from "../../src/renderer/plate";
import { RENDERER_VERSION } from "../../src/renderer/version";

type Rendered = Awaited<ReturnType<typeof render>>;

// The charts bundle uses production components only.
const charts = readFixtureBundles().charts;
const post = (path: string, body: unknown) =>
  handle(new Request(`http://127.0.0.1${path}`, { method: "POST", body: JSON.stringify(body) }));

describe("Plate handler", () => {
  test("renders a bundle with its chart ids and exactly one noindex robots tag", async () => {
    const res = await post("/render", charts);
    expect(res.status).toBe(200);
    const r = (await res.json()) as Rendered;
    expect(r.rendererVersion).toBe(RENDERER_VERSION);
    expect(r.files["index.html"].match(/<meta name="robots"/g)).toHaveLength(1);
    expect(r.files["index.html"]).toContain('<meta name="robots" content="noindex">');
    expect(r.files["index.md"]).toContain(RENDERER_VERSION);
    expect(r.charts).toContain("services");
    expect(r.files["charts/services.csv"]).toBeString();
    expect(r.refsUsed).toContain("dp_aapl_sales_services_fy2024");
  }, 60_000); // the first call loads the component library

  test("writes the index robots tag only when asked", async () => {
    const r = (await (await post("/render", { ...charts, indexStatus: "index" })).json()) as Rendered;
    expect(r.files["index.html"]).toContain('<meta name="robots" content="index, follow">');
  });

  test("renders the same input to the same bytes", async () => {
    const a = await (await post("/render", charts)).text();
    const b = await (await post("/render", charts)).text();
    expect(b).toBe(a);
  });

  test("check accepts valid OpenUI Lang and lists its components", () => {
    const r = check(charts.oui);
    expect(r.errors).toEqual([]);
    expect(r.components).toContain("KeyFigures");
  });

  test("check reports a parse error and an invalid prop as Copy validator errors", () => {
    expect(check('root = Page([x])\nx = NoSuchThing("a")').errors[0]?.rule).toBe("openui_parse");
    const props = check(
      'root = Page([f])\nf = KeyFigures([a, b])\na = KeyFigure("A", "391.0")\nb = KeyFigure("B", "dp_b")',
    );
    expect(props.errors.length).toBeGreaterThan(0);
    expect(props.errors.every((e) => e.rule === "props")).toBe(true);
    expect(props.errors[0]?.where).toStartWith("oui:a");
  });

  test("refuses bad requests and maps a contract failure to 422", async () => {
    expect((await post("/render", { oui: 1 })).status).toBe(400);
    expect((await post("/render", { oui: "root = Page([])" })).status).toBe(400);
    const bad = await post("/render", { ...charts, data: { version: 1 } });
    expect(bad.status).toBe(422);
    expect(((await bad.json()) as { error: { code: string } }).error.code).toBe("invalid-data");
    expect((await handle(new Request("http://127.0.0.1/nope"))).status).toBe(404);
  });
});

test("the server binds 127.0.0.1 on a free port, says so on its first line and exits when its stdin closes", async () => {
  const script = fileURLToPath(new URL("../../src/renderer/plate.ts", import.meta.url));
  const child = Bun.spawn([process.execPath, script, "--port", "0", "--lifeline"], { stdin: "pipe", stdout: "pipe" });
  const reader = child.stdout.getReader();
  let first = "";
  while (!first.includes("\n")) {
    const { value, done } = await reader.read();
    if (done) break;
    first += new TextDecoder().decode(value);
  }
  const { port, renderer } = JSON.parse(first.split("\n")[0] as string) as { port: number; renderer: string };
  expect(renderer).toBe(RENDERER_VERSION);
  const health = await fetch(`http://127.0.0.1:${port}/health`);
  expect(await health.json()).toEqual({ renderer: RENDERER_VERSION });
  child.stdin.end();
  expect(await child.exited).toBe(0);
}, 60_000);
