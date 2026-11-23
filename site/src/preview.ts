// Local preview of a draft curation, for `news home preview` (#20; ADR 0010, section 6). Compiled in only when the
// build was made with NEWS_PREVIEW=1, so a production build has none of this code, and it fails closed at run time too:
// outside NEWS_ENV=local the Worker refuses to start, and the handlers answer only to a loopback host.
//   PUT /__preview/curation     a curation.v1 body: held in this process's memory, and read instead of the live one
//   DELETE /__preview/curation  forgets the draft
// Every response of a preview server is `no-store` and `noindex`, skips the edge cache, and carries
// `X-News-Preview: <sha256 of the draft's bytes>` while a draft is held.
import { sha256Hex } from "@news/worker-shared/crypto";
import { BodyTooLargeError, readBodyCapped } from "@news/worker-shared/http";
import type { Curation } from "@news/worker-shared/records";
import { check, schemas } from "@news/worker-shared/validate";
import type { SiteData, SiteEnv } from "./data";

const MAX_DRAFT_BYTES = 256 * 1024;
let draft: { curation: Curation; sha256: string } | undefined;

/** Throws unless this is a local run. Called before the first request is served. */
export function assertLocalPreview(env: Pick<SiteEnv, "NEWS_ENV">): void {
  if (env.NEWS_ENV !== "local") throw new Error("the preview build is refused outside NEWS_ENV=local");
}

/** The data layer with the draft in place of the live curation while one is held. */
export const withDraft = (data: SiteData): SiteData => ({
  get reader() {
    return data.reader;
  },
  get shards() {
    return data.shards;
  },
  curation: async () => draft?.curation ?? data.curation(),
  pageJson: (...args) => data.pageJson(...args),
  pageText: (...args) => data.pageText(...args),
});

const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);

const text = (status: number, body: string) =>
  new Response(body, { status, headers: { "content-type": "text/plain; charset=utf-8" } });

/** PUT and DELETE /__preview/curation. */
export async function handleDraft(request: Request, env: Pick<SiteEnv, "NEWS_ENV">): Promise<Response> {
  if (env.NEWS_ENV !== "local" || !LOOPBACK.has(new URL(request.url).hostname)) return text(404, "Not found");
  if (request.method === "DELETE") {
    draft = undefined;
    return new Response(null, { status: 204 });
  }
  if (request.method !== "PUT")
    return new Response("Method not allowed", { status: 405, headers: { allow: "PUT, DELETE" } });
  let body: string;
  try {
    body = await readBodyCapped(request, MAX_DRAFT_BYTES);
  } catch (e) {
    return e instanceof BodyTooLargeError ? text(413, "the draft is too large") : text(400, "the body is not UTF-8");
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(body);
  } catch {
    return text(400, "the body is not JSON");
  }
  const checked = check<Curation>(schemas.validateCuration, parsed);
  if (!checked.ok)
    return text(400, `the draft is not curation.v1: ${checked.issues.map((i) => `${i.path} ${i.message}`).join("; ")}`);
  draft = { curation: checked.value, sha256: await sha256Hex(body) };
  return new Response(null, { status: 204 });
}

/** A preview response: never cached, never indexed, and marked with the draft it was rendered from. */
export function decorate(response: Response): Response {
  const out = new Response(response.body, response);
  out.headers.set("cache-control", "no-store");
  out.headers.set("x-robots-tag", "noindex");
  if (draft) out.headers.set("x-news-preview", draft.sha256);
  else out.headers.delete("x-news-preview");
  return out;
}
