// Local telemetry dashboard (Phase 8). Listens on localhost only.
import { resolvePort } from "./port";

const resolved = resolvePort(process.env.NEWS_DASHBOARD_PORT);
if ("error" in resolved) {
  console.error(resolved.error);
  process.exit(1);
}
const { port } = resolved;
Bun.serve({
  hostname: "127.0.0.1",
  port,
  fetch: () =>
    new Response("News dashboard: coming in Phase 8\n", { headers: { "content-type": "text/plain; charset=utf-8" } }),
});
console.log(`dashboard on http://127.0.0.1:${port}`);
