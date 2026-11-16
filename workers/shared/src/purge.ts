// Cloudflare cache purge by URL, batched and paced to stay inside the API's purge limits.
// Single-file purge accepts up to 100 URLs per call on paid plans and 30 on Free; we send at most 30 per call and
// wait between calls, so one publish (a page plus its hubs, usually under 60 URLs) needs two or three calls.

export interface Purger {
  /** Purges every URL (deduplicated). Throws PurgeError when the CDN refused. Returns the URLs purged. */
  purge(urls: string[]): Promise<string[]>;
}

export class PurgeError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
    this.name = "PurgeError";
  }
}

export interface CloudflarePurgerConfig {
  zoneId: string;
  /** API token with only the Zone > Cache Purge permission on this zone. */
  token: string;
  fetch?: (request: Request) => Promise<Response>;
  batchSize?: number;
  /** Pause between calls. Default 250 ms. */
  pauseMs?: number;
  sleep?: (ms: number) => Promise<void>;
}

const ZONE_ID = /^[0-9a-f]{32}$/;
/** URLs per purge call (the free plan's maximum): one subrequest each. */
export const PURGE_BATCH = 30;

export class CloudflarePurger implements Purger {
  readonly #config: Required<Omit<CloudflarePurgerConfig, "fetch" | "sleep">>;
  readonly #fetch: (request: Request) => Promise<Response>;
  readonly #sleep: (ms: number) => Promise<void>;

  constructor(config: CloudflarePurgerConfig) {
    if (!ZONE_ID.test(config.zoneId)) throw new Error("invalid Cloudflare zone ID");
    if (!config.token) throw new Error("Cloudflare purge token is not set");
    this.#config = { batchSize: PURGE_BATCH, pauseMs: 250, ...config };
    if (this.#config.batchSize < 1 || this.#config.batchSize > 100) throw new Error("purge batch size must be 1–100");
    this.#fetch = config.fetch ?? ((r) => fetch(r));
    this.#sleep = config.sleep ?? ((ms) => new Promise((r) => setTimeout(r, ms)));
  }

  async purge(urls: string[]): Promise<string[]> {
    const unique = [...new Set(urls)];
    const { batchSize, pauseMs, zoneId, token } = this.#config;
    for (let i = 0; i < unique.length; i += batchSize) {
      if (i > 0) await this.#sleep(pauseMs);
      const files = unique.slice(i, i + batchSize);
      for (let attempt = 0; ; attempt++) {
        const res = await this.#fetch(
          new Request(`https://api.cloudflare.com/client/v4/zones/${zoneId}/purge_cache`, {
            method: "POST",
            headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
            body: JSON.stringify({ files }),
            signal: AbortSignal.timeout(10_000),
          }),
        );
        if (res.ok) break;
        await res.body?.cancel();
        // Rate limited: wait once (at most 2 s, as the Worker request is still open), then give up.
        if (res.status === 429 && attempt === 0) {
          const retry = Number(res.headers.get("retry-after"));
          await this.#sleep(Math.min(Number.isFinite(retry) && retry > 0 ? retry * 1000 : 1000, 2000));
          continue;
        }
        throw new PurgeError(`cache purge failed with HTTP ${res.status}`, res.status);
      }
    }
    return unique;
  }
}

/** Records purges instead of calling Cloudflare: tests, and local runs without a zone. */
export class RecordingPurger implements Purger {
  readonly calls: string[][] = [];
  /** Make the next purge throw (tests of the 502 path). */
  failNext = false;

  async purge(urls: string[]): Promise<string[]> {
    const unique = [...new Set(urls)];
    if (this.failNext) {
      this.failNext = false;
      throw new PurgeError("cache purge failed (test)", 500);
    }
    this.calls.push(unique);
    return unique;
  }

  get all(): string[] {
    return this.calls.flat();
  }
}
