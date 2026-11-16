// A stateless, read-only MCP server over Streamable HTTP, MCP specification revision 2026-07-28.
//
// Modern requests (2026-07-28) carry their version, client info and capabilities in params._meta and mirror the
// method (and tool name) in headers; there is no handshake and no session. For clients of earlier revisions the
// server is dual-era: `initialize` (2025-11-25 / 2025-06-18) is answered without minting a session, and their
// tools/list and tools/call are served the same way. One endpoint, POST /mcp; GET and DELETE are 405.
import { clientIp, readBodyCapped, SECURITY_HEADERS } from "@news/worker-shared/http";
import { type Deps, UnavailableError } from "./deps";
import { cachedRun } from "./cache";
import { CAPS, TOOL_BY_NAME, TOOL_LIST, type ToolOutput } from "./tools";

export const MODERN_VERSION = "2026-07-28";
export const LEGACY_VERSIONS = ["2025-11-25", "2025-06-18"] as const;
export const SUPPORTED_VERSIONS = [MODERN_VERSION, ...LEGACY_VERSIONS];
export const SERVER_INFO = { name: "news-mcp", title: "News", version: "1.0.0" };
export const MCP_PATH = "/mcp";
const MAX_REQUEST_BYTES = 64 * 1024;
const META = "io.modelcontextprotocol/";

export const ERR = {
  parse: -32700,
  invalidRequest: -32600,
  methodNotFound: -32601,
  invalidParams: -32602,
  internal: -32603,
  headerMismatch: -32020,
  unsupportedVersion: -32022,
  /** Application-defined, outside the JSON-RPC reserved range. */
  rateLimited: -31001,
} as const;

const INSTRUCTIONS =
  "Read-only access to News: visual, fully sourced pages on company, economic and science reports. " +
  "Use search_pages to find pages, get_page for a page's Markdown or metadata, get_data for the figures behind a page " +
  "or chart (exactly as published, with source IDs), get_entity for a company, country or institution and its pages, " +
  "and list_categories for the category tree. Figures are never converted between currencies, units or periods.";

type Id = string | number;

class RpcError extends Error {
  constructor(
    readonly code: number,
    message: string,
    readonly http: number,
    readonly data?: unknown,
  ) {
    super(message);
  }
}

const headers = (extra: Record<string, string> = {}) => ({
  "content-type": "application/json",
  ...SECURITY_HEADERS,
  ...extra,
});

function rpcError(id: Id | null, e: RpcError, extra: Record<string, string> = {}): Response {
  const body = {
    jsonrpc: "2.0",
    ...(id === null ? {} : { id }),
    error: { code: e.code, message: e.message, ...(e.data === undefined ? {} : { data: e.data }) },
  };
  return new Response(JSON.stringify(body), { status: e.http, headers: headers(extra) });
}

/** Decodes the `=?base64?…?=` sentinel used for header values that are not plain ASCII. */
export function decodeHeaderValue(value: string): string | null {
  const m = /^=\?base64\?([A-Za-z0-9+/]*={0,2})\?=$/.exec(value);
  if (!m) return value;
  try {
    const bytes = Uint8Array.from(atob(m[1] as string), (c) => c.charCodeAt(0));
    return new TextDecoder("utf-8", { fatal: true, ignoreBOM: false }).decode(bytes);
  } catch {
    return null;
  }
}

interface Message {
  jsonrpc: "2.0";
  id?: Id;
  method: string;
  params?: Record<string, unknown>;
}

function parseMessage(raw: unknown): Message {
  if (Array.isArray(raw)) throw new RpcError(ERR.invalidRequest, "Batch requests are not supported", 400);
  const m = raw as Partial<Message> | null;
  if (!m || typeof m !== "object" || m.jsonrpc !== "2.0" || typeof m.method !== "string" || m.method.length > 100)
    throw new RpcError(ERR.invalidRequest, "Invalid JSON-RPC request", 400);
  if ("id" in m && !(typeof m.id === "string" || (typeof m.id === "number" && Number.isInteger(m.id))))
    throw new RpcError(ERR.invalidRequest, "id must be a string or an integer", 400);
  if (m.params !== undefined && (typeof m.params !== "object" || m.params === null || Array.isArray(m.params)))
    throw new RpcError(ERR.invalidParams, "params must be an object", 400);
  return m as Message;
}

type Era = { kind: "modern" } | { kind: "legacy"; version: string };

/** Works out the request's protocol era and enforces the 2026-07-28 header and _meta rules for modern requests. */
function negotiate(request: Request, msg: Message): Era {
  const header = request.headers.get("mcp-protocol-version");
  const meta = (msg.params?._meta ?? undefined) as Record<string, unknown> | undefined;
  const metaVersion = meta?.[`${META}protocolVersion`];
  const unsupported = (requested: unknown) =>
    new RpcError(ERR.unsupportedVersion, "Unsupported protocol version", 400, {
      supported: SUPPORTED_VERSIONS,
      requested,
    });

  if (metaVersion === undefined && header !== null && (LEGACY_VERSIONS as readonly string[]).includes(header))
    return { kind: "legacy", version: header };
  if (header === null) {
    if (metaVersion === undefined)
      throw new RpcError(ERR.headerMismatch, "Missing MCP-Protocol-Version header", 400, {
        supported: SUPPORTED_VERSIONS,
      });
    throw new RpcError(ERR.headerMismatch, "Header mismatch: MCP-Protocol-Version header is missing", 400);
  }
  if (metaVersion === undefined) {
    if (header === MODERN_VERSION)
      throw new RpcError(ERR.invalidParams, `Missing required _meta field ${META}protocolVersion`, 400);
    throw unsupported(header);
  }
  if (typeof metaVersion !== "string")
    throw new RpcError(ERR.invalidParams, `${META}protocolVersion must be a string`, 400);
  if (metaVersion !== header)
    throw new RpcError(
      ERR.headerMismatch,
      `Header mismatch: MCP-Protocol-Version '${header.slice(0, 40)}' does not match _meta`,
      400,
    );
  if (metaVersion !== MODERN_VERSION) throw unsupported(metaVersion);
  const caps = meta?.[`${META}clientCapabilities`];
  if (caps === undefined || typeof caps !== "object" || caps === null || Array.isArray(caps))
    throw new RpcError(ERR.invalidParams, `Missing required _meta field ${META}clientCapabilities`, 400);

  const method = request.headers.get("mcp-method");
  if (method === null) throw new RpcError(ERR.headerMismatch, "Header mismatch: Mcp-Method header is missing", 400);
  if (method !== msg.method)
    throw new RpcError(ERR.headerMismatch, "Header mismatch: Mcp-Method does not match the body", 400);
  if (msg.method === "tools/call" || msg.method === "resources/read" || msg.method === "prompts/get") {
    const nameHeader = request.headers.get("mcp-name");
    const bodyName = msg.params?.name ?? msg.params?.uri;
    if (nameHeader === null) throw new RpcError(ERR.headerMismatch, "Header mismatch: Mcp-Name header is missing", 400);
    if (decodeHeaderValue(nameHeader) !== bodyName)
      throw new RpcError(ERR.headerMismatch, "Header mismatch: Mcp-Name does not match the body", 400);
  }
  return { kind: "modern" };
}

const enc = new TextEncoder();

async function callTool(
  deps: Deps,
  params: Record<string, unknown> | undefined,
  origin: string,
): Promise<Record<string, unknown>> {
  const name = params?.name;
  if (typeof name !== "string") throw new RpcError(ERR.invalidParams, "params.name must be a string", 400);
  const tool = TOOL_BY_NAME.get(name);
  if (!tool) throw new RpcError(ERR.invalidParams, `Unknown tool: ${name.slice(0, 100)}`, 400);
  const args = params?.arguments ?? {};
  const failure = (text: string) => ({ content: [{ type: "text", text }], isError: true });
  if (!tool.input(args)) {
    const issues = (tool.input.errors ?? [])
      .slice(0, 5)
      .map((e) => `${e.instancePath || "arguments"} ${e.message ?? e.keyword}`);
    return failure(`Invalid arguments for ${name}: ${issues.join("; ")}`);
  }
  let out: ToolOutput;
  try {
    const outcome = await cachedRun(deps.cache, origin, name, args as Record<string, unknown>, () =>
      tool.run(deps, args as Record<string, unknown>),
    );
    if (!outcome.ok) return failure(outcome.message);
    out = outcome.out;
  } catch (e) {
    if (e instanceof UnavailableError) throw new RpcError(ERR.internal, "The server is not fully configured", 500);
    deps.log.error("tool_failed", {
      tool: name,
      error: e instanceof Error ? `${e.name}: ${e.message}`.slice(0, 200) : "unknown",
    });
    throw new RpcError(ERR.internal, "Internal error", 500);
  }
  const result = {
    content: [{ type: "text", text: out.text ?? JSON.stringify(out.structured) }],
    structuredContent: out.structured,
    isError: false,
  };
  if (enc.encode(JSON.stringify(result)).length > CAPS.resultBytes)
    return failure(`The result of ${name} is too large.`);
  return result;
}

async function dispatch(deps: Deps, msg: Message, era: Era, origin: string): Promise<Record<string, unknown>> {
  switch (msg.method) {
    case "server/discover":
      if (era.kind !== "modern") break;
      return {
        supportedVersions: SUPPORTED_VERSIONS,
        capabilities: { tools: { listChanged: false } },
        instructions: INSTRUCTIONS,
        ttlMs: 3_600_000,
        cacheScope: "public",
      };
    case "ping":
      return {};
    case "tools/list":
      return era.kind === "modern"
        ? { tools: TOOL_LIST, ttlMs: 3_600_000, cacheScope: "public" }
        : { tools: TOOL_LIST };
    case "tools/call":
      return callTool(deps, msg.params, origin);
  }
  throw new RpcError(ERR.methodNotFound, `Method not found: ${msg.method.slice(0, 100)}`, 404);
}

function legacyInitialize(msg: Message): Record<string, unknown> {
  const requested = msg.params?.protocolVersion;
  const version =
    typeof requested === "string" && (LEGACY_VERSIONS as readonly string[]).includes(requested)
      ? requested
      : LEGACY_VERSIONS[0];
  return {
    protocolVersion: version,
    capabilities: { tools: { listChanged: false } },
    serverInfo: SERVER_INFO,
    instructions: INSTRUCTIONS,
  };
}

function originAllowed(deps: Deps, request: Request): boolean {
  const origin = request.headers.get("origin");
  return origin === null || deps.allowedOrigins.has(origin);
}

export async function handleMcp(deps: Deps, request: Request): Promise<Response> {
  // DNS-rebinding defence: a browser Origin must be one we know.
  if (!originAllowed(deps, request)) return rpcError(null, new RpcError(ERR.invalidRequest, "Forbidden origin", 403));
  if (request.method === "GET" || request.method === "DELETE")
    return new Response(null, { status: 405, headers: { allow: "POST", ...SECURITY_HEADERS } });
  if (request.method !== "POST")
    return new Response(null, { status: 405, headers: { allow: "POST", ...SECURITY_HEADERS } });

  if (deps.limiter) {
    const { success } = await deps.limiter.limit({ key: clientIp(request) });
    if (!success)
      return rpcError(null, new RpcError(ERR.rateLimited, "Rate limit exceeded; retry in a minute", 429), {
        "retry-after": "60",
      });
  }

  const contentType = (request.headers.get("content-type") ?? "").split(";")[0]?.trim().toLowerCase();
  if (contentType !== "application/json")
    return rpcError(null, new RpcError(ERR.invalidRequest, "Content-Type must be application/json", 415));

  let raw: unknown;
  try {
    raw = JSON.parse(await readBodyCapped(request, MAX_REQUEST_BYTES));
  } catch (e) {
    if (e instanceof Error && e.name === "BodyTooLargeError")
      return rpcError(null, new RpcError(ERR.invalidRequest, `Request larger than ${MAX_REQUEST_BYTES} bytes`, 413));
    return rpcError(null, new RpcError(ERR.parse, "Parse error", 400));
  }

  let msg: Message;
  try {
    msg = parseMessage(raw);
  } catch (e) {
    const id = raw && typeof raw === "object" && "id" in raw ? ((raw as { id: unknown }).id as Id) : null;
    return rpcError(typeof id === "string" || typeof id === "number" ? id : null, e as RpcError);
  }

  // A notification (no id): accepted, never answered.
  if (msg.id === undefined) return new Response(null, { status: 202, headers: SECURITY_HEADERS });
  const id = msg.id;

  try {
    if (msg.method === "initialize") {
      // Legacy handshake only; 2026-07-28 clients never send it. No session is created or returned.
      const metaVersion = (msg.params?._meta as Record<string, unknown> | undefined)?.[`${META}protocolVersion`];
      if (metaVersion !== undefined)
        throw new RpcError(
          ERR.methodNotFound,
          `initialize is not part of ${MODERN_VERSION}; send requests directly`,
          404,
          {
            supported: SUPPORTED_VERSIONS,
          },
        );
      return Response.json({ jsonrpc: "2.0", id, result: legacyInitialize(msg) }, { headers: headers() });
    }
    const era = negotiate(request, msg);
    const result = await dispatch(deps, msg, era, new URL(request.url).origin);
    const body =
      era.kind === "modern"
        ? { ...result, resultType: "complete", _meta: { [`${META}serverInfo`]: SERVER_INFO } }
        : result;
    return new Response(JSON.stringify({ jsonrpc: "2.0", id, result: body }), { status: 200, headers: headers() });
  } catch (e) {
    if (e instanceof RpcError) return rpcError(id, e);
    deps.log.error("unhandled", { error: e instanceof Error ? e.name : "unknown" });
    return rpcError(id, new RpcError(ERR.internal, "Internal error", 500));
  }
}
