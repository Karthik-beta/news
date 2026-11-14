// news-search: Public search API over D1 (Phase 6). Read-only.
// Placeholder until Phase 6. Answers /health so local tooling can check it runs.
export default {
  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/health") {
      return Response.json({ worker: "news-search", status: "ok" });
    }
    return Response.json({ error: "Not implemented yet (Phase 6)" }, { status: 501 });
  },
} satisfies ExportedHandler;
