// Small helpers shared by the measurement scripts. Runtime-neutral (Node or Bun), so Vitest can use them too.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { gzipSync } from "node:zlib";

export const RUNS = 50;
export const WARMUP = 5;

export function percentile(sorted: number[], p: number): number {
  const i = Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1));
  return sorted[i] as number;
}

export function timings(ms: number[]) {
  const s = [...ms].sort((a, b) => a - b);
  const round = (n: number) => Math.round(n * 1000) / 1000;
  return {
    runs: s.length,
    p50Ms: round(percentile(s, 50)),
    p95Ms: round(percentile(s, 95)),
    minMs: round(s[0] as number),
    maxMs: round(s[s.length - 1] as number),
  };
}

export function sizes(text: string | Uint8Array) {
  const bytes = typeof text === "string" ? Buffer.from(text) : Buffer.from(text);
  const kb = (n: number) => Math.round((n / 1024) * 100) / 100;
  return { rawKB: kb(bytes.length), gzipKB: kb(gzipSync(bytes, { level: 9 }).length) };
}

/** Times `fn` RUNS times after WARMUP untimed calls; `firstMs` is the very first (cold) call. */
export async function bench(fn: () => unknown | Promise<unknown>) {
  const t0 = performance.now();
  await fn();
  const firstMs = Math.round((performance.now() - t0) * 1000) / 1000;
  for (let i = 1; i < WARMUP; i++) await fn();
  const ms: number[] = [];
  for (let i = 0; i < RUNS; i++) {
    const t = performance.now();
    await fn();
    ms.push(performance.now() - t);
  }
  return { firstMs, ...timings(ms) };
}

/** Merges `patch` into the JSON results file, so each script owns its own keys. */
export function writeResults(file: string, patch: Record<string, unknown>) {
  let current: Record<string, unknown> = {};
  try {
    current = JSON.parse(readFileSync(file, "utf8"));
  } catch {}
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, `${JSON.stringify({ ...current, ...patch }, null, 2)}\n`);
}
