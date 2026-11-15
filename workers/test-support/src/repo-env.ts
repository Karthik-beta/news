// Settings from the repository's root `.env`, for test runs started from a package directory (`workers/api`):
// Bun only loads the `.env` of the directory it starts in, so a custom NEWS_DYNAMODB_* value would otherwise be
// missed and the run would probe the default port. Both the Vitest config and the global setup call this, so neither
// depends on process.env being shared between them.
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** The repository's root `.env` (workers/test-support/src → repo root). */
export const ROOT_ENV_PATH = fileURLToPath(new URL("../../../.env", import.meta.url));

/** Parse dotenv text: `KEY=value` lines, comments and blanks skipped, one pair of matching quotes removed. */
export function parseDotenv(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split(/\r?\n/)) {
    const m = line.match(/^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=(.*)$/);
    if (!m?.[1] || m[1] in out) continue;
    const raw = (m[2] ?? "").trim();
    const quoted = raw.length >= 2 && (raw[0] === '"' || raw[0] === "'") && raw.at(-1) === raw[0];
    out[m[1]] = quoted ? raw.slice(1, -1) : raw;
  }
  return out;
}

/** Root `.env` values overlaid by the process's own non-empty values (an exported variable always wins). */
export function repoEnv(
  processEnv: Record<string, string | undefined> = process.env,
  path: string = ROOT_ENV_PATH,
): Record<string, string> {
  const merged = existsSync(path) ? parseDotenv(readFileSync(path, "utf8")) : {};
  for (const [k, v] of Object.entries(processEnv)) if (v) merged[k] = v;
  return merged;
}
