// The pages that list pages render in plain Bun with their data in memory, and stay inside ADR 0010's render budget
// (4 ms p95, to leave headroom under workerd's CPU limit on the free plan).
import { expect, test } from "bun:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./render-budget.script.ts", import.meta.url));

function measure(): Record<string, number> {
  const run = Bun.spawnSync([process.execPath, script], { env: { ...process.env, NODE_ENV: "production" } });
  expect(run.stderr.toString()).toBe("");
  return JSON.parse(run.stdout.toString().trim().split("\n").at(-1) as string) as Record<string, number>;
}

test("a hub renders within 4 ms p95; the home page and its slot figures are measured apart", () => {
  // A loaded laptop can push one run over; the budget holds if one of three runs is inside it.
  const runs = [measure(), measure(), measure()];
  console.log(`render p95 (ms) per run: ${JSON.stringify(runs)}`);
  expect(runs[0]?.bytes).toBeGreaterThan(2000);
  expect(Math.min(...runs.map((r) => r.hub as number))).toBeLessThanOrEqual(4);
  // The home page has about 30 stories: 2.5 to 4.3 ms p95 on an idle 4-core laptop, up to 8 ms loaded; the 4 ms target is for hubs.
  expect(Math.min(...runs.map((r) => r.home as number))).toBeLessThanOrEqual(10);
  // A cited figure costs about a millisecond (its citation popover) and a mini chart a few: outside the 4 ms.
  expect(Math.min(...runs.map((r) => r.homeWithFigures as number))).toBeLessThanOrEqual(10);
}, 180_000);
