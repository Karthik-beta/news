// Timestamps. Every stored timestamp is Date.toISOString() (UTC, always three fractional digits), so string order is
// time order: "…:00Z" and "…:00.500Z" would otherwise sort the wrong way round ('.' < 'Z').

const TIMESTAMP = /^([0-9]{4})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2}):([0-9]{2})(?:\.([0-9]{1,3}))?Z$/;

/**
 * The canonical form of a UTC timestamp ('2026-11-12T10:00:00Z' → '2026-11-12T10:00:00.000Z'), or null when it is not
 * one or names a day that does not exist (2026-02-31, 2026-02-29).
 */
export function normalizeTimestamp(value: string): string | null {
  const m = TIMESTAMP.exec(value);
  if (!m) return null;
  const [y, mo, d, h, mi, s] = m.slice(1, 7).map(Number) as [number, number, number, number, number, number];
  const ms = Number((m[7] ?? "").padEnd(3, "0"));
  const date = new Date(Date.UTC(y, mo - 1, d, h, mi, s, ms));
  if (
    y < 1000 ||
    date.getUTCFullYear() !== y ||
    date.getUTCMonth() !== mo - 1 ||
    date.getUTCDate() !== d ||
    date.getUTCHours() !== h ||
    date.getUTCMinutes() !== mi ||
    date.getUTCSeconds() !== s
  )
    return null;
  return date.toISOString();
}
