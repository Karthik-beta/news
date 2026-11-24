// Plate, the warm local render server (ADR 0007; Bureau Design §5.7: the `render` pool's two slots share one server).
// The Bureau service and `news render` / `news ai accept` call it over HTTP on 127.0.0.1, so React, OpenUI and the
// component library load once instead of once per page.
//
//   GET  /health                                               -> {renderer}
//   POST /render {oui, data, sources, head, indexStatus}       -> {rendererVersion, files, charts, refsUsed,
//                                                                  sourcesUsed, problems}
//   POST /check  {oui}                                         -> {errors: [{rule, where, message}], components,
//                                                                  literals: [{where, text, number}]}
//
// Plate is local-only: a request whose Host is not 127.0.0.1, localhost or [::1] with a port is refused (a web
// page cannot reach it through DNS rebinding), and a POST must say Content-Type: application/json (a cross-site
// form cannot send that without a preflight).
//
// Run: `bun src/renderer/plate.ts [--port N] [--lifeline]`. Port 0 picks a free port; the first stdout line is
// {"port": N, "renderer": "x.y.z"} once the library is loaded. With --lifeline the server exits when its stdin
// closes, so a server started by a CLI call never outlives it.
import type { ElementNode } from "@openuidev/react-lang";
import { newsLibrary } from "../components/library";
import { checkProps, type PageHead, parsePage, RenderError, renderPage } from "./render";
import { RENDERER_VERSION } from "./version";

export interface CheckError {
  rule: "openui_parse" | "props";
  where: string;
  message: string;
}

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
const fail = (status: number, code: string, message: string, details: unknown = null) =>
  json({ error: { code, message, details } }, status);

/** "where: message" lines after a RenderError's first line. */
function lines(e: RenderError, rule: CheckError["rule"]): CheckError[] {
  return e.message
    .split("\n")
    .slice(1)
    .map((l) => l.trim())
    .filter(Boolean)
    .map((l) => {
      const at = l.indexOf(": ");
      return at > 0
        ? { rule, where: `oui:${l.slice(0, at)}`, message: l.slice(at + 2) }
        : { rule, where: "oui", message: l };
    });
}

export interface Literal {
  where: string;
  text: string;
  number: boolean;
}

/** Every string and number the parser read, under its statement: what the Copy lints check (C4, banned words). */
function literalsOf(value: unknown, where: string, out: Literal[]): Literal[] {
  if (typeof value === "string") out.push({ where, text: value, number: false });
  else if (typeof value === "number") out.push({ where, text: String(value), number: true });
  else if (Array.isArray(value)) for (const v of value) literalsOf(v, where, out);
  else if (value && typeof value === "object") {
    const node = value as ElementNode;
    if (node.type === "element") {
      for (const v of Object.values(node.props)) literalsOf(v, `oui:${node.statementId ?? node.typeName}`, out);
    } else for (const v of Object.values(value)) literalsOf(v, where, out);
  }
  return out;
}

function componentsOf(value: unknown, out: Set<string>): Set<string> {
  if (Array.isArray(value)) for (const v of value) componentsOf(v, out);
  else if (value && typeof value === "object" && (value as ElementNode).type === "element") {
    const node = value as ElementNode;
    out.add(node.typeName);
    for (const v of Object.values(node.props)) componentsOf(v, out);
  }
  return out;
}

/** OpenUI parse and props against the component specs: the two Copy validators that need the library. */
export function check(oui: string): { errors: CheckError[]; components: string[]; literals: Literal[] } {
  const { library } = newsLibrary();
  let root: ElementNode;
  try {
    root = parsePage(oui, library);
  } catch (e) {
    if (e instanceof RenderError) return { errors: lines(e, "openui_parse"), components: [], literals: [] };
    throw e;
  }
  const components = [...componentsOf(root, new Set())].sort();
  const literals = literalsOf(root, "oui", []);
  try {
    checkProps(root, library);
  } catch (e) {
    if (e instanceof RenderError) return { errors: lines(e, "props"), components, literals };
    throw e;
  }
  return { errors: [], components, literals };
}

interface RenderRequest {
  oui: string;
  data: unknown;
  sources: unknown;
  head: PageHead;
  indexStatus?: "index" | "noindex";
}

export function render(req: RenderRequest) {
  const result = renderPage(
    { oui: req.oui, data: req.data, sources: req.sources, head: req.head },
    { indexStatus: req.indexStatus ?? "noindex" },
  );
  return {
    rendererVersion: result.rendererVersion,
    files: result.files,
    charts: result.charts.map((c) => c.statementId),
    refsUsed: result.refsUsed,
    sourcesUsed: result.sourcesUsed,
    problems: result.problems,
  };
}

const LOOPBACK_HOST = /^(127\.0\.0\.1|localhost|\[::1\]):\d{1,5}$/i;

export async function handle(request: Request): Promise<Response> {
  const path = new URL(request.url).pathname;
  if (!LOOPBACK_HOST.test(request.headers.get("host") ?? ""))
    return fail(403, "forbidden", "Plate answers loopback hosts only (127.0.0.1, localhost or [::1] with a port)");
  if (request.method === "GET" && path === "/health") return json({ renderer: RENDERER_VERSION });
  if (request.method !== "POST" || (path !== "/render" && path !== "/check"))
    return fail(404, "not_found", `${request.method} ${path} is not a Plate endpoint`);
  const type = request.headers.get("content-type")?.split(";")[0]?.trim().toLowerCase();
  if (type !== "application/json") return fail(415, "unsupported_media_type", "Content-Type must be application/json");
  let body: Record<string, unknown>;
  try {
    body = (await request.json()) as Record<string, unknown>;
  } catch {
    return fail(400, "bad_request", "the body is not JSON");
  }
  if (typeof body?.oui !== "string") return fail(400, "bad_request", "oui must be a string");
  try {
    if (path === "/check") return json(check(body.oui));
    const head = body.head as PageHead | undefined;
    if (!head || ![head.flyTitle, head.headline, head.summary].every((s) => typeof s === "string"))
      return fail(400, "bad_request", "head needs flyTitle, headline and summary strings");
    return json(render(body as unknown as RenderRequest));
  } catch (e) {
    if (e instanceof RenderError) return fail(422, e.code, e.message, e.details);
    return fail(500, "internal", e instanceof Error ? e.message : String(e));
  }
}

if (import.meta.main) {
  const at = process.argv.indexOf("--port");
  const port = Number(at > 0 ? process.argv[at + 1] : (process.env.NEWS_PLATE_PORT ?? 0));
  newsLibrary().library.toJSONSchema(); // load and build the library before taking requests
  const server = Bun.serve({ hostname: "127.0.0.1", port, fetch: handle, maxRequestBodySize: 8 * 1024 * 1024 });
  console.log(JSON.stringify({ port: server.port, renderer: RENDERER_VERSION }));
  if (process.argv.includes("--lifeline")) {
    process.stdin.on("end", () => process.exit(0));
    process.stdin.on("close", () => process.exit(0));
    process.stdin.resume();
  }
}
