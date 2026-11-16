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

/** Values from .env that setup's child processes (compose, `news db migrate`, doctor) must see. */
export const CHILD_ENV_KEYS = [
  "NEWS_PG_PASSWORD",
  "NEWS_CH_PASSWORD",
  "DATABASE_URL",
  // The migration must target the schema `bun run work` and the other scripts will use.
  "NEWS_DB_SCHEMA",
  "NEWS_DYNAMODB_PORT",
  "NEWS_DYNAMODB_ENDPOINT",
  "NEWS_DYNAMODB_REGION",
  "NEWS_DYNAMODB_ACCESS_KEY_ID",
  "NEWS_DYNAMODB_SECRET_ACCESS_KEY",
] as const;

/**
 * Copy the final .env values into `env` (normally process.env) where it has none or an empty one. An exported
 * shell value is kept, as composeEnv keeps it. Setup only ever fills empty .env values, so a non-empty value in
 * `env` is never a stale copy of the file.
 */
export function reloadChildEnv(text: string, env: Record<string, string | undefined>): void {
  for (const k of CHILD_ENV_KEYS) if (!env[k]) env[k] = getEnvValue(text, k);
}

/** Every `KEY=value` in dotenv text, read literally (no `$` expansion), first duplicate wins. */
export function parseEnv(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of text.matchAll(/^([A-Za-z_][A-Za-z0-9_]*)=/gm)) {
    const key = m[1] as string;
    if (!(key in out)) out[key] = getEnvValue(text, key);
  }
  return out;
}

/**
 * The environment for `docker compose`: the .env values read literally, with the shell's environment laid over them,
 * so an exported variable wins (as in workers/test-support/src/repo-env.ts); an exported empty value does not.
 * Bun expands `$` in .env values even inside single quotes, and Compose expands them in an --env-file, so a password
 * such as `ab$cd` would reach the container as `ab`. Values passed through the process environment are not expanded.
 * `env` must be the shell's own environment: the wrapper runs under `bun --no-env-file`, so Bun's expanded copies
 * of the .env values never reach it.
 */
export function composeEnv(text: string, env: Record<string, string | undefined>): Record<string, string> {
  const out = parseEnv(text);
  for (const [k, v] of Object.entries(env)) {
    if (v || (v === "" && !(k in out))) out[k] = v;
  }
  return out;
}

/**
 * Why this process cannot build the Compose environment, or undefined when it can. Without `--no-env-file`, Bun has
 * already loaded .env into process.env, `$`-expanded, and its copies are indistinguishable from exported values.
 */
export function noEnvFileProblem(execArgv: string[], script: string): string | undefined {
  if (execArgv.includes("--no-env-file")) return undefined;
  return `Run this through its package.json script, or as \`bun --no-env-file ${script}\`: Bun must not load .env first.`;
}

/** .env keys whose value Bun read differently from the file (a `$` in the value), so Bun-run tools see the wrong one. */
export function expandedKeys(text: string, env: Record<string, string | undefined>): string[] {
  return Object.entries(parseEnv(text))
    .filter(([k, v]) => v.includes("$") && env[k] !== undefined && env[k] !== v)
    .map(([k]) => k);
}
