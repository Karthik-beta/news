// component-index.json: one line per component in the page writer's library, built from the component folders
// at `bun run build` (ADR 0001: at most 4,000 tokens in all, 24 per entry; the integration test checks the built
// file). `news components` serves the same lines into the cached prompt prefix.
import type { APIRoute } from "astro";
import { buildComponentIndex, componentSpecs, indexBudgetProblems } from "../components/library";
import { specs } from "../components/registry.generated";

export const GET: APIRoute = () => {
  const index = buildComponentIndex(componentSpecs(specs));
  const problems = indexBudgetProblems(index);
  if (problems.length > 0)
    throw new Error(`component-index.json is over budget (ADR 0001):\n  ${problems.join("\n  ")}`);
  return new Response(`${JSON.stringify(index, null, 2)}\n`, {
    headers: { "Content-Type": "application/json; charset=utf-8" },
  });
};
