// Pure helpers for reading and generating the local `.env` (used by setup.ts, covered by scripts/tests).
import { randomBytes } from "node:crypto";

/** A URL-safe random secret for local passwords (base64url, 32 characters). */
export const newSecret = () => randomBytes(24).toString("base64url");

/** Value of `KEY=value` in dotenv text ("" when the key is missing or empty). First match wins. */
export function getEnvValue(text: string, key: string): string {
  return text.match(new RegExp(`^${key}=(.*)$`, "m"))?.[1]?.trim() ?? "";
}

/** Set `KEY=value`: replaces the first existing line, otherwise appends one. */
export function setEnvValue(text: string, key: string, value: string): string {
  return new RegExp(`^${key}=`, "m").test(text)
    ? text.replace(new RegExp(`^${key}=.*$`, "m"), `${key}=${value}`)
    : `${text.trimEnd()}\n${key}=${value}\n`;
}

/** Fill in the generated local passwords and DATABASE_URL. A value that is already set is never changed. */
export function withLocalSecrets(text: string, secret: () => string = newSecret): string {
  let out = text;
  if (!getEnvValue(out, "NEWS_PG_PASSWORD")) out = setEnvValue(out, "NEWS_PG_PASSWORD", secret());
  if (!getEnvValue(out, "NEWS_CH_PASSWORD")) out = setEnvValue(out, "NEWS_CH_PASSWORD", secret());
  if (!getEnvValue(out, "DATABASE_URL")) {
    const port = getEnvValue(out, "NEWS_PG_PORT") || "5433";
    out = setEnvValue(
      out,
      "DATABASE_URL",
      `postgres://news:${getEnvValue(out, "NEWS_PG_PASSWORD")}@127.0.0.1:${port}/news`,
    );
  }
  return out;
}
