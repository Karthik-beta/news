// Money as the source states it (design/content-style.md, "Currencies"). The currency is never converted: the
// formatter only chooses how to write the source's own ISO 4217 code.
import type { Decimal } from "./decimal";
import { compactNumber, formatNumber, MINUS, SCALE_WORD, type Scale } from "./number";

/** Prefix symbols from content-style.md. Anything not listed is written as its ISO code and a space. */
export const CURRENCY_PREFIX: Readonly<Record<string, string>> = {
  USD: "$",
  EUR: "€",
  GBP: "£",
  INR: "₹",
  KRW: "₩",
  AUD: "A$",
  CAD: "C$",
  HKD: "HK$",
  TWD: "NT$",
  SGD: "S$",
};

/** Yen and yuan use ¥ only when the chart subtitle names the currency; otherwise the word follows the number. */
export const CURRENCY_WORD: Readonly<Record<string, string>> = { JPY: "yen", CNY: "yuan" };

export type MoneyStyle =
  /** Key figures, prose, labels: "$391.0bn", "₹2.1trn", "CHF 4.5bn". */
  | "compact"
  /** The source's own digits and scale word, for citation popovers and as-reported tables: "$391,035 million". */
  | "as-reported";

export interface MoneyOptions {
  style?: MoneyStyle;
  /** The scale the value is stated in ("millions"). Default "units". */
  scale?: Scale;
  /** Digits after the point; compact defaults to one, never more than the source gives. */
  decimals?: number;
  /** Digits after the point the source is precise to, in the value's own scale. */
  sourceDecimals?: number;
  /** True when the nearby subtitle names the currency, which allows ¥ for yen and yuan. */
  currencyNamed?: boolean;
}

function isoCode(currency: string): string {
  if (!/^[A-Z]{3}$/.test(currency)) throw new TypeError(`Not an ISO 4217 currency code: ${JSON.stringify(currency)}`);
  return currency;
}

/** How the currency is written around a number: a prefix ("$", "CHF "), or a word after it (" yen"). */
export function currencyAffixes(currency: string, currencyNamed = false): { prefix: string; suffix: string } {
  const code = isoCode(currency);
  const symbol = CURRENCY_PREFIX[code];
  if (symbol) return { prefix: symbol, suffix: "" };
  const word = CURRENCY_WORD[code];
  if (word) return currencyNamed ? { prefix: "¥", suffix: "" } : { prefix: "", suffix: ` ${word}` };
  return { prefix: `${code} `, suffix: "" };
}

/** Puts the minus before the currency: "−$5.7bn", "−CHF 4.5bn". */
function attach(number: string, prefix: string, suffix: string): string {
  const negative = number.startsWith(MINUS);
  const digits = negative ? number.slice(MINUS.length) : number;
  return `${negative ? MINUS : ""}${prefix}${digits}${suffix}`;
}

/** "$391.0bn" (compact) or "$391,035 million" (as reported). */
export function formatMoney(value: Decimal, currency: string, options: MoneyOptions = {}): string {
  const { prefix, suffix } = currencyAffixes(currency, options.currencyNamed);
  const scale = options.scale ?? "units";
  if (options.style === "as-reported") {
    const word = SCALE_WORD[scale];
    const number = formatNumber(value, { decimals: options.decimals, sourceDecimals: options.sourceDecimals });
    return attach(`${number}${word ? ` ${word}` : ""}`, prefix, suffix);
  }
  const c = compactNumber(value, { scale, decimals: options.decimals, sourceDecimals: options.sourceDecimals });
  return attach(`${c.number}${c.suffix}`, prefix, suffix);
}
