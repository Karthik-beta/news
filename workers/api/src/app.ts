// The write API's request handling: routing, authentication, body limits, idempotency, errors.
//
// Public:        GET|HEAD /health, POST|OPTIONS /v1/reports
// Secret key:    everything else under /v1 (Authorization: Bearer <NEWS_WRITE_API_KEY>)
// Every write needs an Idempotency-Key. Contract: contracts/cloud/write-api.openapi.json.
import { secretsEqual } from "@news/worker-shared/crypto";
import { OutcomeUnknownError, ThrottledError } from "@news/worker-shared/dynamo/table";
import { BodyTooLargeError, clientIp, isJsonContentType, json, readBodyCapped } from "@news/worker-shared/http";
import { PurgeError } from "@news/worker-shared/purge";
import { executeRedirects, executeTaxonomy, prepareRedirects, prepareTaxonomy } from "./catalog";
import { type Deps, NotConfiguredError } from "./deps";
import { ApiError } from "./errors";
import { executeCuration, executeRollback, prepareCuration, prepareRollback, recoverCuration } from "./home";
import { claim, complete, lookup, readKey, release, requestHash } from "./idempotency";
import { Fence, type HeldLock, lockPage, unlockPage } from "./lock";
import {
  checkBundle,
  checkIndex,
  checkWithdraw,
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

/** What an operation knows about the request it runs for. */
export interface OpContext {
  /** SHA-256 of the Idempotency-Key: a retry of this request carries the same one. */
  requestId: string;
  /** SHA-256 of method, path and body: the same key with another body is another request. */
  requestHash: string;
  /** Deadline, storage timeouts and the lock check for the commit (lock.ts). */
  fence: Fence;
}

interface WriteRoute<P, C = unknown> {
  operation: string;
  maxBody: number;
  /**
   * Finds what this request (same key and body) already committed, before any check: the result skips check and
   * prepare and goes straight to execute, which finishes the work. Read-only.
   */
  recover?: (deps: Deps, seen: { keyHash: string; requestHash: string }) => Promise<P | undefined>;
  /** Checks the body without reading or writing anything; runs before any lock is taken. Default: the body as is. */
  check?: (params: string[], body: unknown) => Promise<C>;
  /** For page operations: the page whose lock is held from before `prepare` reads it until the operation is done. */
  lockPage?: (params: string[]) => string;
  prepare(deps: Deps, params: string[], checked: C): Promise<P>;
  execute(deps: Deps, prepared: P, ctx: OpContext): Promise<OpResult>;
}

interface Route {
  pattern: RegExp;
  methods: Record<string, (deps: Deps, request: Request, params: string[], url: URL) => Promise<Response>>;
}

const write =
  <P, C>(route: WriteRoute<P, C>) =>
  (deps: Deps, request: Request, params: string[], url: URL) =>
    handleWrite(deps, request, params, url, route);

const ROUTES: Route[] = [
  {
    pattern: new RegExp(`^/v1/pages/${PAGE_ID}$`),
    methods: {
      PUT: write({
        operation: "put_page",
        // Measured: a typical bundle (100–300 KB) checks in 2–4 ms of CPU, a 1 MiB one in about 8 ms (JSON.parse,
        // SHA-256 of every file), inside the free plan's 10 ms. Bigger bundles belong in fewer, smaller files.
        maxBody: 1 * MiB,
        lockPage: ([id]) => id as string,
        check: ([id], b) => checkBundle(id as string, b),
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
        lockPage: ([id]) => id as string,
        check: (_p, b) => checkWithdraw(b),
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
        lockPage: ([id]) => id as string,
        check: (_p, b) => checkIndex(b),
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
        recover: recoverCuration,
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

/**
 * Clients that ran into the failed-attempt limit, per limiter binding and isolate, with the time their block ends.
 * They are refused before any key comparison, so once limited even a right guess gets nothing until the block ends.
 */
const blockedBy = new WeakMap<RateLimit, Map<string, number>>();
const BLOCK_MS = 60_000;
const MAX_BLOCKED = 10_000;
/** Loggers that already reported a missing write key: production has one logger, so that is once per isolate. */
const keyMissingLogged = new WeakSet<Deps["log"]>();

async function authenticate(deps: Deps, request: Request): Promise<void> {
  const limiter = deps.limiters.authFailure;
  const ip = clientIp(request);
  const now = deps.now().getTime();
  const blocked = limiter && blockedBy.get(limiter);
  const until = blocked?.get(ip);
  const limitedError = () =>
    new ApiError("rate_limited", "too many failed attempts", undefined, {
      "retry-after": String(Math.max(1, Math.ceil(((until ?? now + BLOCK_MS) - now) / 1000))),
    });
  if (until !== undefined) {
    if (until > now) throw limitedError();
    blocked?.delete(ip);
  }

  const header = request.headers.get("authorization") ?? "";
  const presented = header.startsWith("Bearer ") ? header.slice(7) : "";
  const expected = deps.writeKey ?? "";
  // Fail closed: with no key (or a weak one) configured, every write is refused. Logged once per isolate.
  const configured = expected.length >= 32;
  if (!configured && !keyMissingLogged.has(deps.log)) {
    keyMissingLogged.add(deps.log);
    deps.log.error("write_key_not_configured");
  }
  const ok = configured && presented.length > 0 && (await secretsEqual(presented, expected));
  if (ok) return;
  if (limiter) {
    const { success } = await limiter.limit({ key: ip });
    if (!success) {
      const map = blocked ?? new Map<string, number>();
      if (map.size >= MAX_BLOCKED) map.clear();
      map.set(ip, now + BLOCK_MS);
      blockedBy.set(limiter, map);
      throw new ApiError("rate_limited", "too many failed attempts", undefined, { "retry-after": "60" });
    }
  }
  throw new ApiError("unauthorized", "a valid write key is required", undefined, {
    "www-authenticate": 'Bearer realm="news"',
  });
}

async function handleWrite<P, C>(
  deps: Deps,
  request: Request,
  params: string[],
  url: URL,
  route: WriteRoute<P, C>,
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

  const recovered = route.recover ? await route.recover(deps, seen) : undefined;
  const checked = recovered !== undefined ? undefined : route.check ? await route.check(params, body) : (body as C);
  // A page operation holds the page lock from before it reads the page, so nothing else changes the page under it.
  const pageLock: HeldLock | undefined = route.lockPage
    ? await lockPage(table, route.lockPage(params), deps.now())
    : undefined;
  // After a write whose outcome is unknown, both locks stay held until they go stale (2 minutes), so the retry runs
  // only once that write has certainly landed or failed. Every operation detects work it already committed.
  let keepLocks = false;
  try {
    const prepared = recovered ?? (await route.prepare(deps, params, checked as C));
    const idem = await claim(table, seen, route.operation, deps.now());
    let result: OpResult;
    try {
      result = await route.execute(deps, prepared, {
        requestId: seen.keyHash,
        requestHash: seen.requestHash,
        fence: new Fence(pageLock, deps.now, deps.storageTimeoutMs),
      });
    } catch (e) {
      if (e instanceof OutcomeUnknownError) keepLocks = true;
      // Any other failure: release the key so the retry runs the operation again, which finishes what is missing.
      else await release(table, idem);
      throw e;
    }
    const responseText = JSON.stringify(result.body);
    try {
      await complete(table, idem, route.operation, deps.now(), result.status, responseText);
    } catch (e) {
      // The work is done; only the replay record is missing, so a retry would redo it (safely). Report success.
      deps.log.warn("idempotency_complete_failed", {
        operation: route.operation,
        error: e instanceof Error ? e.name : "unknown",
      });
    }
    return json(result.body, result.status);
  } catch (e) {
    if (e instanceof OutcomeUnknownError) keepLocks = true;
    throw e;
  } finally {
    if (pageLock && !keepLocks) await unlockPage(table, pageLock);
  }
}

function toResponse(deps: Deps, e: unknown, extra: Record<string, string> = {}): Response {
  if (e instanceof ApiError) return e.toResponse(extra);
  if (e instanceof ThrottledError)
    return new ApiError("throttled", "the database is busy; retry with the same Idempotency-Key", undefined, {
      "retry-after": "2",
    }).toResponse(extra);
  if (e instanceof OutcomeUnknownError) {
    deps.log.error("outcome_unknown", { message: e.message.slice(0, 200) });
    return new ApiError(
      "outcome_unknown",
      "a write may or may not have been applied; retry with the same Idempotency-Key in 2 minutes",
      undefined,
      { "retry-after": "120" },
    ).toResponse(extra);
  }
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

  try {
    // Everything under /v1/ is authenticated before routing, including the route and method checks: an
    // unauthenticated caller learns nothing about which paths exist.
    if (path.startsWith("/v1/")) await authenticate(deps, request);
    const route = ROUTES.map((r) => ({ r, m: r.pattern.exec(path) })).find((x) => x.m);
    if (!route?.m) throw new ApiError("not_found", "not found");
    const handler = route.r.methods[method];
    if (!handler) {
      const allow = Object.keys(route.r.methods).join(", ");
      throw new ApiError("method_not_allowed", `use ${allow}`, undefined, { allow });
    }
    return await handler(deps, request, route.m.slice(1), url);
  } catch (e) {
    return toResponse(deps, e);
  }
}
