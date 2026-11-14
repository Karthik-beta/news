// news-api: Write API used by news publish (Phase 6). Requires the secret key; never public-writable.
// Placeholder until Phase 6. Answers /health so local tooling can check it runs.
export default {
  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/health") {
      return Response.json({ worker: "news-api", status: "ok" });
    }
    return Response.json({ error: "Not implemented yet (Phase 6)" }, { status: 501 });
  },
} satisfies ExportedHandler;
