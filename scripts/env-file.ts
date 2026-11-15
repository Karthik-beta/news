// Pure helpers for reading and generating the local `.env` (used by setup.ts, covered by scripts/tests).
import { randomBytes } from "node:crypto";

/** A URL-safe random secret for local passwords (base64url, 32 characters). */
export const newSecret = () => randomBytes(24).toString("base64url");

/**
 * Value of `KEY=value` in dotenv text ("" when the key is missing or empty). First match wins.
 * One pair of matching surrounding quotes is removed, as Docker Compose and Bun do, so
 * `KEY="x"` and `KEY='x'` both read as `x`.
 */
export function getEnvValue(text: string, key: string): string {
  const raw = text.match(new RegExp(`^${key}=(.*)$`, "m"))?.[1]?.trim() ?? "";
  const quoted = raw.length >= 2 && (raw[0] === '"' || raw[0] === "'") && raw.at(-1) === raw[0];
  return quoted ? raw.slice(1, -1) : raw;
}

/** Set `KEY=value`: replaces the first existing line, otherwise appends one. The value is written literally. */
export function setEnvValue(text: string, key: string, value: string): string {
  return new RegExp(`^${key}=`, "m").test(text)
    ? // A function replacer, so `$&`, `$$` and friends in the value are not expanded.
      text.replace(new RegExp(`^${key}=.*$`, "m"), () => `${key}=${value}`)
    : `${text.trimEnd()}\n${key}=${value}\n`;
}

/** Fill in the generated local passwords and DATABASE_URL. A value that is already set is never changed. */
export function withLocalSecrets(text: string, secret: () => string = newSecret): string {
  let out = text;
  if (!getEnvValue(out, "NEWS_PG_PASSWORD")) out = setEnvValue(out, "NEWS_PG_PASSWORD", secret());
  if (!getEnvValue(out, "NEWS_CH_PASSWORD")) out = setEnvValue(out, "NEWS_CH_PASSWORD", secret());
  if (!getEnvValue(out, "DATABASE_URL")) {
    const port = getEnvValue(out, "NEWS_PG_PORT") || "5433";
    // Percent-encode the password so `@`, `/`, `:`, `#` and the like cannot break the URL.
    const password = encodeURIComponent(getEnvValue(out, "NEWS_PG_PASSWORD"));
    out = setEnvValue(out, "DATABASE_URL", `postgres://news:${password}@127.0.0.1:${port}/news`);
  }
  return out;
}
