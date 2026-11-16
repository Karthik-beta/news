// A change, written the house way (design/content-style.md, "Change words"): in words for key figures ("Up 2.0%
// on fiscal 2023", "Down 3.4%", "Unchanged") or as a signed figure in compact columns ("+12.9%", "−7.1%").
// Never coloured: the sign is carried by the word or the sign, not by red or green.
import {
  formatChange,
  formatMoney,
  formatPercent,
  formatPoints,
  formatQuantity,
  formatSigned,
  unitKind,
} from "../../format";
import type { ResolvedValue } from "../../renderer/data/resolve";

type OkValue = Extract<ResolvedValue, { ok: true }>;

/** The size of a change in its own unit: "2.0%", "1.9 points", "$5.6bn". */
export function changeSize(v: OkValue): (x: string) => string {
  const sourceDecimals = v.sourceDecimals ?? undefined;
  if (v.currency) return (x) => formatMoney(x, v.currency as string, { scale: v.scale ?? "units", sourceDecimals });
  const kind = unitKind(v.unit);
  if (kind === "percent") return (x) => formatPercent(x, { sourceDecimals });
  if (kind === "pp" || kind === "bp") return (x) => formatPoints(x, kind, { sourceDecimals });
  return (x) => formatQuantity(x, v.unit, { scale: v.scale ?? "units", sourceDecimals });
}

/** "Up 2.0% on fiscal 2023" (words) or "+2.0%" (signed). `against` is the period label, verbatim. */
export function changeText(v: OkValue, style: "words" | "signed", against?: string): string {
  const size = changeSize(v);
  return style === "words" ? formatChange(v.value, size, { against }) : formatSigned(v.value, size);
}
