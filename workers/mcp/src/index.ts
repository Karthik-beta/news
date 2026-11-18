// news-mcp: read-only, stateless MCP server (spec revision 2026-07-28) on POST /mcp. Tools: search_pages, get_page,
// list_categories, get_entity, get_data. Rate limited per client. Protocol handling: server.ts; tools: tools.ts.
import { json } from "@news/worker-shared/http";
import { depsFromEnv, type Env } from "./deps";
import { handleMcp, MCP_PATH } from "./server";

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const path = new URL(request.url).pathname;
    if (path === "/health" && (request.method === "GET" || request.method === "HEAD"))
      return json({ worker: "news-mcp", status: "ok" });
    if (path === MCP_PATH) return handleMcp(depsFromEnv(env), request);
    return json({ error: { code: "not_found", message: "not found; the MCP endpoint is POST /mcp" } }, 404);
  },
} satisfies ExportedHandler<Env>;
