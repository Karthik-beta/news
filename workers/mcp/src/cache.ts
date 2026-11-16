// Edge cache for tool results (the Cache API, per Cloudflare location). Identical calls within a short window are
// answered without touching DynamoDB, R2 or D1, so repeated or scripted calls cannot spend the publishing table's
// read units. "Not found" answers are cached too; "being updated, try again" is not. Results may be up to the TTL
// old: a published update or a withdrawal reaches MCP clients within a minute (five for the category tree).
import { sha256Hex } from "@news/worker-shared/crypto";
import { RetryLaterError, ToolError, type ToolOutput } from "./tools";

export const TTL_SECONDS: Record<string, number> = {
  search_pages: 60,
  get_page: 60,
  get_data: 60,
  get_entity: 60,
  list_categories: 300,
};

export type Outcome = { ok: true; out: ToolOutput } | { ok: false; message: string; retryLater?: true };

/** JSON with object keys sorted, so `{a, b}` and `{b, a}` share an entry. */
function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object")
    return `{${Object.keys(value)
      .sort()
      .map((k) => `${JSON.stringify(k)}:${canonical((value as Record<string, unknown>)[k])}`)
      .join(",")}}`;
  return JSON.stringify(value);
}

/**
 * Runs the tool, or answers from the cache. `origin` is the MCP Worker's own origin: entries live under a path no
 * client can request (`/mcp/_cache/…`), in the Worker's zone. Without a cache (tests, workers.dev) it just runs.
 */
export async function cachedRun(
  cache: Cache | undefined,
  origin: string,
  name: string,
  args: Record<string, unknown>,
  run: () => Promise<ToolOutput>,
): Promise<Outcome> {
  const execute = async (): Promise<Outcome> => {
    try {
      return { ok: true, out: await run() };
    } catch (e) {
      if (e instanceof RetryLaterError) return { ok: false, message: e.message, retryLater: true };
      if (e instanceof ToolError) return { ok: false, message: e.message };
      throw e;
    }
  };
  const ttl = TTL_SECONDS[name];
  if (!cache || !ttl) return execute();
  const key = `${origin}/mcp/_cache/${name}/${await sha256Hex(canonical(args))}`;
  const hit = await cache.match(key).catch(() => undefined);
  if (hit) return (await hit.json()) as Outcome;
  const outcome = await execute();
  if (!outcome.ok && outcome.retryLater) return outcome;
  await cache
    .put(
      key,
      new Response(JSON.stringify(outcome), {
        headers: { "content-type": "application/json", "cache-control": `public, max-age=${ttl}` },
      }),
    )
    .catch(() => {});
  return outcome;
}
