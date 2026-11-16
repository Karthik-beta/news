// Reader error reports: public intake (POST /v1/reports) and the authenticated pull for Errata (GET /v1/reports).
// Spam controls (ADR 0006): a body and text size limit, per-IP and per-location rate limits, a global daily cap
// enforced by the database, a honeypot field, and a check that the page is published. Reports are stored in their own
// D1 database (report-store.ts) and the page check reads the search index, so the intake never touches the DynamoDB
// table that publishing writes to. The stored row has the report text, the page, the optional "where" and e-mail,
// never the IP address or any other request detail; it expires after 180 days. Report text is untrusted reader input.
import { randomHex } from "@news/worker-shared/crypto";
import { clientIp, isJsonContentType, json, readBodyCapped } from "@news/worker-shared/http";
import type { ReportKind } from "@news/worker-shared/records";
import { normalizeTimestamp } from "@news/worker-shared/time";
import { check, type Issue, schemas } from "@news/worker-shared/validate";
import type { Deps } from "./deps";
import { ApiError } from "./errors";
import type { StoredReport } from "./report-store";

export const REPORT_MAX_BYTES = 8 * 1024;
/** Text and "where" are capped in UTF-8 bytes too: the schema counts characters, and one can be 4 bytes. */
export const TEXT_MAX_BYTES = 4000;
export const WHERE_MAX_BYTES = 600;
const RETENTION_SECONDS = 180 * 24 * 3600;
/**
 * Reports received in the last minute are listed on a later pull. A report's receipt time is set before it is
 * written, so without this a slow write could land behind a next_after the reader already moved past.
 */
export const SETTLE_MS = 60_000;
const ACCEPTED = { status: "received" } as const;

interface Intake {
  page_id: string;
  kind?: ReportKind;
  text: string;
  where?: string;
  email?: string;
  lang?: string;
  website?: string;
}

/** CORS headers for the site's own origin only; other origins get none (and a 403 on POST). */
export function corsHeaders(deps: Deps, request: Request): Record<string, string> {
  if (request.headers.get("origin") !== deps.siteOrigin) return {};
  return {
    "access-control-allow-origin": deps.siteOrigin,
    "access-control-allow-methods": "POST, OPTIONS",
    "access-control-allow-headers": "content-type",
    "access-control-max-age": "86400",
    vary: "origin",
  };
}

async function limited(limiter: RateLimit | undefined, key: string): Promise<boolean> {
  if (!limiter) return false;
  const { success } = await limiter.limit({ key });
  return !success;
}

const utf8 = new TextEncoder();

/** Seconds until the next UTC midnight, when the daily cap resets. */
const untilTomorrow = (now: Date) =>
  String(Math.ceil((Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() + 1) - now.getTime()) / 1000));

export async function receiveReport(deps: Deps, request: Request): Promise<Response> {
  const origin = request.headers.get("origin");
  if (origin !== null && origin !== deps.siteOrigin)
    throw new ApiError("forbidden", "reports are accepted from the site only");
  const cors = corsHeaders(deps, request);

  // Rate limits first: the cheapest refusal. The IP is the limiter's key only; it is not stored or logged.
  if (
    (await limited(deps.limiters.reportIp, clientIp(request))) ||
    (await limited(deps.limiters.reportGlobal, "global"))
  )
    throw new ApiError("rate_limited", "too many reports; try again in a minute", undefined, {
      "retry-after": "60",
      ...cors,
    });

  if (!isJsonContentType(request.headers.get("content-type")))
    throw new ApiError("unsupported_content_type", "send the report as application/json", undefined, cors);
  let body: unknown;
  try {
    body = JSON.parse(await readBodyCapped(request, REPORT_MAX_BYTES));
  } catch (e) {
    if (e instanceof Error && e.name === "BodyTooLargeError")
      throw new ApiError("payload_too_large", `a report is at most ${REPORT_MAX_BYTES} bytes`, undefined, cors);
    throw new ApiError("invalid_json", "the body is not valid JSON", undefined, cors);
  }

  // Honeypot: the site hides `website` from people. A filled one is a bot: answer as if accepted, store nothing.
  if (
    body &&
    typeof body === "object" &&
    typeof (body as Intake).website === "string" &&
    (body as Intake).website !== ""
  )
    return json(ACCEPTED, 202, cors);

  const r = check<Intake>(schemas.validateReportIntake, body);
  if (!r.ok) throw new ApiError("invalid_request", "the report does not match the report schema", r.issues, cors);
  const intake = r.value;
  const tooLong: Issue[] = [];
  if (utf8.encode(intake.text).length > TEXT_MAX_BYTES)
    tooLong.push({ code: "report.too_long", message: `text is over ${TEXT_MAX_BYTES} bytes`, path: "/text" });
  if (intake.where !== undefined && utf8.encode(intake.where).length > WHERE_MAX_BYTES)
    tooLong.push({ code: "report.too_long", message: `where is over ${WHERE_MAX_BYTES} bytes`, path: "/where" });
  if (tooLong.length > 0) throw new ApiError("invalid_request", "the report is too long", tooLong, cors);

  // The page must be published: its search row exists exactly while it is (removed on withdrawal).
  const page = await deps.search.findPage(intake.page_id);
  if (!page) throw new ApiError("unknown_page", "no published page has this ID", undefined, cors);

  const now = deps.now();
  const createdAt = now.toISOString();
  const store = deps.reports();
  const full = () =>
    new ApiError("rate_limited", "the site has received all the reports it takes today; try tomorrow", undefined, {
      "retry-after": untilTomorrow(now),
      ...cors,
    });
  // Once this isolate has seen the cap reached today, refuse without asking the database again.
  if (store.fullDay === createdAt.slice(0, 10)) throw full();
  const id = `r_${randomHex(16)}`;
  const report: StoredReport = {
    id,
    page_id: intake.page_id,
    page_path: page.path,
    kind: intake.kind ?? "other",
    text: intake.text,
    ...(intake.where ? { where: intake.where } : {}),
    ...(intake.email ? { email: intake.email } : {}),
    lang: intake.lang ?? page.lang,
    created_at: createdAt,
    expires_at: Math.floor(now.getTime() / 1000) + RETENTION_SECONDS,
  };
  if (!(await store.add(report, deps.reportDailyCap))) {
    store.fullDay = createdAt.slice(0, 10);
    deps.log.warn("report_daily_cap_reached", { cap: deps.reportDailyCap });
    throw full();
  }
  return json(ACCEPTED, 202, cors);
}

/** GET /v1/reports?after=<timestamp or next_after>&limit=<1..200>: reports in order of receipt. */
export async function listReports(deps: Deps, url: URL): Promise<Response> {
  const now = deps.now();
  const nowSeconds = Math.floor(now.getTime() / 1000);
  const raw = url.searchParams.get("after") ?? new Date(now.getTime() - 30 * 86_400_000).toISOString();
  if (!schemas.validateReportsCursor(raw))
    throw new ApiError("invalid_request", "`after` must be a UTC timestamp or a next_after value");
  // A next_after is '<created_at>#<id>' (strictly after it); a bare timestamp means "at or after". The timestamp is
  // normalised to the stored form, so '…:00Z' and '…:00.5Z' compare correctly with '…:00.500Z'.
  const [stamp = "", afterId = ""] = raw.split("#");
  const after = normalizeTimestamp(stamp);
  if (!after) throw new ApiError("invalid_request", "`after` is not a real UTC time");
  const limitParam = url.searchParams.get("limit") ?? "100";
  if (!/^[0-9]{1,3}$/.test(limitParam) || Number(limitParam) < 1 || Number(limitParam) > 200)
    throw new ApiError("invalid_request", "`limit` must be 1–200");
  const limit = Number(limitParam);
  // Never look back further than the 180-day retention.
  const oldest = new Date((nowSeconds - RETENTION_SECONDS) * 1000).toISOString();
  const store = deps.reports();
  await store.deleteExpired(nowSeconds);
  const rows = await store.list({
    // biome-ignore lint/complexity/useMathMinMax: ISO strings; Math.max would turn them into NaN.
    after: after < oldest ? oldest : after,
    afterId: after < oldest ? "" : afterId,
    receivedBefore: new Date(now.getTime() - SETTLE_MS).toISOString(),
    liveAt: nowSeconds,
    limit,
  });
  const last = rows[rows.length - 1];
  return json({
    reports: rows.map(({ expires_at: _exp, ...r }) => r),
    next_after: rows.length >= limit && last ? `${last.created_at}#${last.id}` : null,
  });
}
