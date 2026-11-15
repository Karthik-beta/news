// The write API's request handling: routing, authentication, body limits, idempotency, errors.
//
// Public:        GET|HEAD /health, POST|OPTIONS /v1/reports
// Secret key:    everything else under /v1 (Authorization: Bearer <NEWS_WRITE_API_KEY>)
// Every write needs an Idempotency-Key. Contract: contracts/cloud/write-api.openapi.json.
import { secretsEqual } from "@news/worker-shared/crypto";
import { ThrottledError } from "@news/worker-shared/dynamo/table";
import { BodyTooLargeError, clientIp, isJsonContentType, json, readBodyCapped } from "@news/worker-shared/http";
import { PurgeError } from "@news/worker-shared/purge";
import { executeRedirects, executeTaxonomy, prepareRedirects, prepareTaxonomy } from "./catalog";
import { type Deps, NotConfiguredError } from "./deps";
import { ApiError } from "./errors";
import { executeCuration, executeRollback, prepareCuration, prepareRollback } from "./home";
import { claim, complete, lookup, readKey, release, requestHash } from "./idempotency";
import {
  executeIndex,
  executePut,
  executeWithdraw,
  type OpResult,
  prepareIndex,
  preparePut,
  prepareWithdraw,
} from "./pages";
import { corsHeaders, listReports, receiveReport } from "./reports";

const MiB = 1024 * 1024;
const PAGE_ID = "([a-z0-9](?:[a-z0-9-]{0,118}[a-z0-9])?)";

interface WriteRoute<P> {
  operation: string;
  maxBody: number;
  prepare(deps: Deps, params: string[], body: unknown): Promise<P>;
  execute(deps: Deps, prepared: P): Promise<OpResult>;
}

interface Route {
  pattern: RegExp;
  methods: Record<string, (deps: Deps, request: Request, params: string[], url: URL) => Promise<Response>>;
}

const write =
  <P>(route: WriteRoute<P>) =>
  (deps: Deps, request: Request, params: string[], url: URL) =>
    handleWrite(deps, request, params, url, route);

const ROUTES: Route[] = [
  {
    pattern: new RegExp(`^/v1/pages/${PAGE_ID}$`),
    methods: {
      PUT: write({
        operation: "put_page",
        maxBody: 5 * MiB,
        prepare: (d, [id], b) => preparePut(d, id as string, b),
        execute: executePut,
      }),
    },
  },
  {
    pattern: new RegExp(`^/v1/pages/${PAGE_ID}/withdraw$`),
    methods: {
      POST: write({
        operation: "withdraw",
        maxBody: 4096,
        prepare: (d, [id], b) => prepareWithdraw(d, id as string, b),
        execute: executeWithdraw,
      }),
    },
  },
  {
    pattern: new RegExp(`^/v1/pages/${PAGE_ID}/index$`),
    methods: {
      POST: write({
        operation: "index_promote",
        maxBody: 4096,
        prepare: (d, [id], b) => prepareIndex(d, id as string, b),
        execute: executeIndex,
      }),
    },
  },
  {
    pattern: /^\/v1\/home\/curation$/,
    methods: {
      PUT: write({
        operation: "home_curation",
        maxBody: 64 * 1024,
        prepare: (d, _p, b) => prepareCuration(d, b),
        execute: executeCuration,
      }),
    },
  },
  {
    pattern: /^\/v1\/home\/curation\/rollback$/,
    methods: {
      POST: write({
        operation: "home_rollback",
        maxBody: 4096,
        prepare: (d, _p, b) => prepareRollback(d, b),
        execute: executeRollback,
      }),
    },
  },
  {
    pattern: /^\/v1\/taxonomy$/,
    methods: {
      PUT: write({
        operation: "taxonomy",
        maxBody: 256 * 1024,
        prepare: (d, _p, b) => prepareTaxonomy(d, b),
        execute: executeTaxonomy,
      }),
    },
  },
  {
    pattern: /^\/v1\/redirects$/,
    methods: {
      PUT: write({
        operation: "redirects",
        maxBody: 256 * 1024,
        prepare: (d, _p, b) => prepareRedirects(d, b),
        execute: executeRedirects,
      }),
    },
  },
  {
    pattern: /^\/v1\/reports$/,
    methods: { GET: async (deps, _r, _p, url) => listReports(deps, url) },
  },
];

async function authenticate(deps: Deps, request: Request): Promise<void> {
  const header = request.headers.get("authorization") ?? "";
  const presented = header.startsWith("Bearer ") ? header.slice(7) : "";
  const expected = deps.writeKey ?? "";
  // Fail closed: with no key (or a weak one) configured, every write is refused.
  const configured = expected.length >= 32;
  if (!configured) deps.log.error("write_key_not_configured");
  const ok = configured && presented.length > 0 && (await secretsEqual(presented, expected));
  if (ok) return;
  if (deps.limiters.authFailure) {
    const { success } = await deps.limiters.authFailure.limit({ key: clientIp(request) });
    if (!success) throw new ApiError("rate_limited", "too many failed attempts", undefined, { "retry-after": "60" });
  }
  throw new ApiError("unauthorized", "a valid write key is required", undefined, {
    "www-authenticate": 'Bearer realm="news"',
  });
}

async function handleWrite<P>(
  deps: Deps,
  request: Request,
  params: string[],
  url: URL,
  route: WriteRoute<P>,
): Promise<Response> {
  if (!isJsonContentType(request.headers.get("content-type")))
    throw new ApiError("unsupported_content_type", "the body must be application/json");
  const key = readKey(request);
  let text: string;
  try {
    text = await readBodyCapped(request, route.maxBody);
  } catch (e) {
    if (e instanceof BodyTooLargeError)
      throw new ApiError("payload_too_large", `the body is larger than ${route.maxBody} bytes`);
    throw new ApiError("invalid_json", "the body is not valid UTF-8");
  }
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    throw new ApiError("invalid_json", "the body is not valid JSON");
  }

  const table = deps.table();
  const seen = await lookup(table, key, await requestHash(request.method, url.pathname, text), deps.now());
  if (seen.kind === "replay") return seen.response;

  const prepared = await route.prepare(deps, params, body);
  const lock = await claim(table, seen, route.operation, deps.now());
  let result: OpResult;
  try {
    result = await route.execute(deps, prepared);
  } catch (e) {
    await release(table, lock);
    throw e;
  }
  const responseText = JSON.stringify(result.body);
  try {
    await complete(table, lock, route.operation, deps.now(), result.status, responseText);
  } catch (e) {
    // The work is done; only the replay record is missing, so a retry would redo it (safely). Report success.
    deps.log.warn("idempotency_complete_failed", {
      operation: route.operation,
      error: e instanceof Error ? e.name : "unknown",
    });
  }
  return json(result.body, result.status);
}

function toResponse(deps: Deps, e: unknown, extra: Record<string, string> = {}): Response {
  if (e instanceof ApiError) return e.toResponse(extra);
  if (e instanceof ThrottledError)
    return new ApiError("throttled", "the database is busy; retry with the same Idempotency-Key", undefined, {
      "retry-after": "2",
    }).toResponse(extra);
  if (e instanceof PurgeError) {
    deps.log.error("purge_failed", { status: e.status });
    return new ApiError(
      "purge_failed",
      "the change was written but the cache purge failed; retry with the same Idempotency-Key",
    ).toResponse(extra);
  }
  if (e instanceof NotConfiguredError) {
    deps.log.error("not_configured", { message: e.message });
    return new ApiError("not_configured", "the service is not fully configured").toResponse(extra);
  }
  // Unknown: log the error class and message, never the request.
  deps.log.error("unhandled", { error: e instanceof Error ? `${e.name}: ${e.message}`.slice(0, 300) : "unknown" });
  return new ApiError("internal", "internal error").toResponse(extra);
}

export async function handle(deps: Deps, request: Request): Promise<Response> {
  let url: URL;
  try {
    url = new URL(request.url);
  } catch {
    return new ApiError("not_found", "not found").toResponse();
  }
  const path = url.pathname;
  const method = request.method;

  if (path === "/health") {
    if (method === "GET" || method === "HEAD") return json({ worker: "news-api", status: "ok" });
    return new ApiError("method_not_allowed", "use GET", undefined, { allow: "GET, HEAD" }).toResponse();
  }

  if (path === "/v1/reports" && (method === "POST" || method === "OPTIONS")) {
    const cors = corsHeaders(deps, request);
    if (method === "OPTIONS") return new Response(null, { status: 204, headers: cors });
    try {
      return await receiveReport(deps, request);
    } catch (e) {
      return toResponse(deps, e, e instanceof ApiError ? {} : cors);
    }
  }

  const route = ROUTES.map((r) => ({ r, m: r.pattern.exec(path) })).find((x) => x.m);
  if (!route?.m) return new ApiError("not_found", "not found").toResponse();
  const handler = route.r.methods[method];
  try {
    // Authenticate before anything else, including the method check: unauthenticated callers learn nothing.
    await authenticate(deps, request);
    if (!handler) {
      const allow = Object.keys(route.r.methods).join(", ");
      throw new ApiError("method_not_allowed", `use ${allow}`, undefined, { allow });
    }
    return await handler(deps, request, route.m.slice(1), url);
  } catch (e) {
    return toResponse(deps, e);
  }
}
