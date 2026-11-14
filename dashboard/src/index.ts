// Local telemetry dashboard (Phase 8). Listens on localhost only.
const port = Number(process.env.NEWS_DASHBOARD_PORT ?? 4400);
Bun.serve({
  hostname: "127.0.0.1",
  port,
  fetch: () =>
    new Response("News dashboard: coming in Phase 8\n", { headers: { "content-type": "text/plain; charset=utf-8" } }),
});
console.log(`dashboard on http://127.0.0.1:${port}`);
