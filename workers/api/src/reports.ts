// Reader error reports: public intake (POST /v1/reports) and the authenticated pull for Errata (GET /v1/reports).
// Spam controls (ADR 0006): a size limit, per-IP and global rate limits, a honeypot field, and a check that the page
// exists. The stored record has the report text, the page and the optional e-mail, never the IP address or any other
// request detail; it expires after 180 days (DynamoDB TTL).
import { randomHex } from "@news/worker-shared/crypto";
import { clientIp, isJsonContentType, json, readBodyCapped } from "@news/worker-shared/http";
import { reportKey, reportPk } from "@news/worker-shared/keys";
import { Reader } from "@news/worker-shared/read";
import type { ReportKind, ReportRecord } from "@news/worker-shared/records";
import { check, schemas } from "@news/worker-shared/validate";
import type { Deps } from "./deps";
import { ApiError } from "./errors";

export const REPORT_MAX_BYTES = 8 * 1024;
const RETENTION_SECONDS = 180 * 24 * 3600;
const ACCEPTED = { status: "received" } as const;

interface Intake {
  page_id: string;
  kind?: ReportKind;
  text: string;
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
  const table = deps.table();
  const page = await new Reader(table).getPage(intake.page_id);
  if (page?.status !== "published")
    throw new ApiError("unknown_page", "no published page has this ID", undefined, cors);

  const now = deps.now();
  const createdAt = now.toISOString();
  const id = `r_${randomHex(16)}`;
  const record: ReportRecord = {
    ...reportKey(createdAt, id),
    type: "report",
    id,
    page_id: page.page_id,
    page_path: page.path,
    kind: intake.kind ?? "other",
    text: intake.text,
    ...(intake.email ? { email: intake.email } : {}),
    lang: intake.lang ?? page.lang,
    created_at: createdAt,
    expires_at: Math.floor(now.getTime() / 1000) + RETENTION_SECONDS,
  };
  await table.put(record as never);
  return json(ACCEPTED, 202, cors);
}

const MAX_MONTHS_BACK = 7;

const monthsBetween = (from: string, to: string): string[] => {
  const months: string[] = [];
  let y = Number(from.slice(0, 4));
  let m = Number(from.slice(5, 7));
  const end = to.slice(0, 7);
  for (let i = 0; i < 24; i++) {
    const key = `${y}-${String(m).padStart(2, "0")}`;
    months.push(key);
    if (key >= end) break;
    m++;
    if (m === 13) {
      m = 1;
      y++;
    }
  }
  return months;
};

/** GET /v1/reports?after=<timestamp or next_after>&limit=<1..200>: reports in order of receipt. */
export async function listReports(deps: Deps, url: URL): Promise<Response> {
  const now = deps.now();
  const defaultAfter = new Date(now.getTime() - 30 * 86_400_000).toISOString();
  const after = url.searchParams.get("after") ?? defaultAfter;
  if (!schemas.validateReportsCursor(after))
    throw new ApiError("invalid_request", "`after` must be a UTC timestamp or a next_after value");
  const limitParam = url.searchParams.get("limit") ?? "100";
  if (!/^[0-9]{1,3}$/.test(limitParam) || Number(limitParam) < 1 || Number(limitParam) > 200)
    throw new ApiError("invalid_request", "`limit` must be 1–200");
  const limit = Number(limitParam);
  const oldest = new Date(now.getTime() - MAX_MONTHS_BACK * 31 * 86_400_000).toISOString();
  // ISO timestamps compare correctly as strings; never look back further than the retention window.
  // biome-ignore lint/complexity/useMathMinMax: these are strings; Math.max would turn them into NaN.
  const start = after < oldest ? oldest : after;

  const table = deps.table();
  const reports: ReportRecord[] = [];
  let last: string | null = null;
  outer: for (const month of monthsBetween(start, now.toISOString())) {
    let cursor: { PK: string; SK: string } | undefined;
    for (let page = 0; page < 50; page++) {
      const res = await table.query({
        pk: reportPk(`${month}-01T00:00:00Z`),
        sk: { between: [start, "9999"] },
        limit: limit - reports.length + 1,
        start: cursor,
      });
      for (const item of res.items as unknown as ReportRecord[]) {
        if (item.SK === start) continue; // `after` is exclusive when it names a report
        reports.push(item);
        last = item.SK;
        if (reports.length >= limit) break outer;
      }
      cursor = res.next;
      if (!cursor) break;
    }
  }
  const full = reports.length >= limit;
  const body = {
    reports: reports.map(({ PK: _pk, SK: _sk, type: _type, expires_at: _exp, ...r }) => r),
    next_after: full ? last : null,
  };
  return json(body);
}
