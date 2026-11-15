// Reads NEWS_DASHBOARD_PORT. Kept apart from index.ts so it can be unit-tested without starting the server.
export const DEFAULT_PORT = 4400;

/** Unset or empty → the default; a whole number 1–65535 → that port; anything else → a one-line error. */
export function resolvePort(raw: string | undefined): { port: number } | { error: string } {
  const value = raw?.trim() ?? "";
  if (value === "") return { port: DEFAULT_PORT };
  const port = Number(value);
  if (!/^\d+$/.test(value) || port < 1 || port > 65535) {
    return { error: `NEWS_DASHBOARD_PORT must be a whole number from 1 to 65535, got ${JSON.stringify(raw)}` };
  }
  return { port };
}
