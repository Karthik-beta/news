// The arithmetic of a waterfall (design/charts.md, "Waterfall"): where each line starts and ends, on exact
// decimals. A total is drawn from zero to its value and resets the running amount; a deduction subtracts its
// value as published (a cost published as 210,352 goes down; a credit published as −20 goes up); an addition adds
// its value as published (other income of 269 goes up; −565 goes down). The running amounts are exact, so the
// check that the lines add up to each total is exact too. Nothing here is shown on the page: labels are written
// from the source values through the shared formatters.

export type StepKind = "total" | "deduct" | "add";

export interface StepInput {
  kind: StepKind;
  /** The value as published, an exact decimal; null when the source has none. */
  value: string | null;
}

export interface Step {
  kind: StepKind;
  /** What the line does to the running amount: a total's value, a deduction's value negated, an addition's value. */
  effect: string | null;
  /** The running amount before and after the line; null where it cannot be known (after a missing value). */
  start: string | null;
  end: string | null;
}

export interface Mismatch {
  /** The index of the total the lines above it do not add up to. */
  index: number;
  /** The running amount the lines give, and the total as published. */
  running: string;
  reported: string;
}

// Exact decimals as (mantissa, exponent): "-12.5" is (-125n, -1).
interface Exact {
  m: bigint;
  e: number;
}

function exact(value: string): Exact {
  const negative = value.startsWith("-");
  const [int = "0", frac = ""] = (negative ? value.slice(1) : value).split(".");
  const m = BigInt(`${int}${frac}` || "0");
  return { m: negative ? -m : m, e: -frac.length };
}

function align(a: Exact, b: Exact): [bigint, bigint, number] {
  const e = Math.min(a.e, b.e);
  return [a.m * 10n ** BigInt(a.e - e), b.m * 10n ** BigInt(b.e - e), e];
}

function write({ m, e }: Exact): string {
  const negative = m < 0n;
  const digits = (negative ? -m : m).toString();
  if (e === 0) return `${negative ? "-" : ""}${digits}`;
  const places = -e;
  const padded = digits.padStart(places + 1, "0");
  const int = padded.slice(0, padded.length - places);
  const frac = padded.slice(padded.length - places).replace(/0+$/, "");
  const body = frac ? `${int}.${frac}` : int;
  return body === "0" ? "0" : `${negative ? "-" : ""}${body}`;
}

/** a + b, exactly. */
export function addExact(a: string, b: string): string {
  const [x, y, e] = align(exact(a), exact(b));
  return write({ m: x + y, e });
}

/** −a, exactly. */
export function negateExact(a: string): string {
  const x = exact(a);
  return write({ m: -x.m, e: x.e });
}

/** The sum of decimals, exactly ("0" for none). */
export function sumExact(values: readonly string[]): string {
  return values.reduce((sum, v) => addExact(sum, v), "0");
}

/** −1, 0 or 1 as a is below, equal to or above b, exactly. */
export function compareExact(a: string, b: string): number {
  const [x, y] = align(exact(a), exact(b));
  return x < y ? -1 : x > y ? 1 : 0;
}

/** The line's effect on the running amount, exactly. */
export function effectOf(kind: StepKind, value: string): string {
  return kind === "deduct" ? negateExact(value) : value;
}

/**
 * Where every line starts and ends. A line before the first total starts from zero. After a missing value the
 * running amount is unknown until the next total, so those lines cannot be placed (start and end null).
 */
export function waterfallSteps(inputs: readonly StepInput[]): Step[] {
  let running: string | null = "0";
  return inputs.map((input) => {
    if (input.value === null) {
      // A missing total or change: nothing to place, and the running amount is lost until the next total.
      running = null;
      return { kind: input.kind, effect: null, start: null, end: null };
    }
    if (input.kind === "total") {
      running = input.value;
      return { kind: "total", effect: input.value, start: "0", end: input.value };
    }
    const effect = effectOf(input.kind, input.value);
    if (running === null) return { kind: input.kind, effect, start: null, end: null };
    const start: string = running;
    running = addExact(start, effect);
    return { kind: input.kind, effect, start, end: running };
  });
}

/**
 * Totals the lines above them do not add up to (exactly): a line left out, or a figure from another table. The
 * first total is where the running amount starts, so it is never a mismatch; a total after an unknown running
 * amount cannot be checked.
 */
export function waterfallMismatches(inputs: readonly StepInput[]): Mismatch[] {
  const steps = waterfallSteps(inputs);
  const out: Mismatch[] = [];
  let running: string | null = null;
  let seenTotal = false;
  inputs.forEach((input, i) => {
    const step = steps[i] as Step;
    if (input.kind === "total") {
      if (seenTotal && running !== null && input.value !== null && compareExact(running, input.value) !== 0) {
        out.push({ index: i, running, reported: input.value });
      }
      seenTotal = true;
      running = input.value;
      return;
    }
    running = step.end;
  });
  return out;
}
