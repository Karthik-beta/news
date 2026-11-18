// Small HTTP helpers shared by the Workers: JSON responses, body reading with a hard size cap.

export const SECURITY_HEADERS: Record<string, string> = {
  "x-content-type-options": "nosniff",
  "cache-control": "no-store",
  "referrer-policy": "no-referrer",
};

export function json(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json; charset=utf-8", ...SECURITY_HEADERS, ...headers },
  });
}

export class BodyTooLargeError extends Error {
  constructor(readonly limit: number) {
    super(`request body is larger than ${limit} bytes`);
    this.name = "BodyTooLargeError";
  }
}

/**
 * Reads the body as UTF-8 text, refusing more than `limit` bytes: up front from Content-Length, and while streaming
 * for chunked bodies or a lying Content-Length. Never buffers more than `limit` bytes.
 */
export async function readBodyCapped(request: Request, limit: number): Promise<string> {
  const declared = request.headers.get("content-length");
  if (declared !== null && (!/^[0-9]+$/.test(declared) || Number(declared) > limit)) throw new BodyTooLargeError(limit);
  if (!request.body) return "";
  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > limit) {
      await reader.cancel();
      throw new BodyTooLargeError(limit);
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const c of chunks) {
    bytes.set(c, offset);
    offset += c.byteLength;
  }
  return new TextDecoder("utf-8", { fatal: true, ignoreBOM: false }).decode(bytes);
}

/** True when the Content-Type is JSON (application/json, optionally with a UTF-8 charset). */
export function isJsonContentType(value: string | null): boolean {
  if (!value) return false;
  const [type, ...params] = value.split(";").map((s) => s.trim().toLowerCase());
  if (type !== "application/json") return false;
  return params.every((p) => p === "" || p === "charset=utf-8" || p === 'charset="utf-8"');
}

/** The client's IP for rate-limit keys only. It is never stored or logged. */
export const clientIp = (request: Request) => request.headers.get("cf-connecting-ip") ?? "unknown";

export const nowIso = (now: Date = new Date()) => now.toISOString();
