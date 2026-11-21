// Dates and periods (design/content-style.md, "Dates and periods"). Dates are parsed from ISO strings by hand, so
// the output never depends on the machine's time zone. A date-time is shown as its UTC calendar date.

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
] as const;
const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"] as const;

export interface CalendarDate {
  year: number;
  month: number; // 1-12
  day: number; // 1-31
}

const DATE = /^(\d{4})-(\d{2})-(\d{2})$/;
const DATE_TIME = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?(Z|[+-]\d{2}:\d{2})$/;
const MONTH_ONLY = /^(\d{4})-(\d{2})$/;

function checked(year: number, month: number, day: number, input: string): CalendarDate {
  const d = new Date(Date.UTC(year, month - 1, day));
  if (d.getUTCFullYear() !== year || d.getUTCMonth() !== month - 1 || d.getUTCDate() !== day) {
    throw new RangeError(`Not a real date: ${JSON.stringify(input)}`);
  }
  return { year, month, day };
}

/** Parses "2024-11-01" or an ISO date-time with an offset ("2026-11-13T10:00:00Z", shown as its UTC date). */
export function parseIsoDate(input: string): CalendarDate {
  const date = DATE.exec(input);
  if (date) return checked(Number(date[1]), Number(date[2]), Number(date[3]), input);
  const dt = DATE_TIME.exec(input);
  if (dt) {
    const ms = Date.parse(input);
    if (Number.isNaN(ms)) throw new RangeError(`Not a real date-time: ${JSON.stringify(input)}`);
    checked(Number(dt[1]), Number(dt[2]), Number(dt[3]), input);
    const d = new Date(ms);
    return { year: d.getUTCFullYear(), month: d.getUTCMonth() + 1, day: d.getUTCDate() };
  }
  throw new TypeError(`Not an ISO date: ${JSON.stringify(input)}`);
}

const monthName = (m: number) => MONTHS[m - 1] as string;

/** "1 November 2024": day month year, no comma. */
export function formatDate(iso: string): string {
  const { year, month, day } = parseIsoDate(iso);
  return `${day} ${monthName(month)} ${year}`;
}

/** "Wednesday 6 November 2024": the home page date line only. */
export function formatDateWithWeekday(iso: string): string {
  const { year, month, day } = parseIsoDate(iso);
  const weekday = WEEKDAYS[new Date(Date.UTC(year, month - 1, day)).getUTCDay()] as string;
  return `${weekday} ${day} ${monthName(month)} ${year}`;
}

/** "1 Nov 2024": tables, lists and axes. */
export function formatShortDate(iso: string): string {
  const { year, month, day } = parseIsoDate(iso);
  return `${day} ${monthName(month).slice(0, 3)} ${year}`;
}

/** "10:42": the time of day of an ISO date-time, in UTC, on the 24-hour clock (the home page's Latest list). */
export function formatTimeOfDay(iso: string): string {
  const dt = DATE_TIME.exec(iso);
  if (!dt) throw new TypeError(`Not an ISO date-time: ${JSON.stringify(iso)}`);
  parseIsoDate(iso);
  const d = new Date(Date.parse(iso));
  const two = (n: number) => String(n).padStart(2, "0");
  return `${two(d.getUTCHours())}:${two(d.getUTCMinutes())}`;
}

/** "October 2026" from "2026-10" or a full date. */
export function formatMonthYear(iso: string): string {
  const m = MONTH_ONLY.exec(iso);
  const { year, month } = m ? checked(Number(m[1]), Number(m[2]), 1, iso) : parseIsoDate(iso);
  return `${monthName(month)} ${year}`;
}

/** "Jan", "Feb" …: month ticks on axes. */
export function formatAxisMonth(iso: string): string {
  const m = MONTH_ONLY.exec(iso);
  const { month } = m ? checked(Number(m[1]), Number(m[2]), 1, iso) : parseIsoDate(iso);
  return monthName(month).slice(0, 3);
}

/**
 * A period label exactly as the source writes it ("fiscal 2024", "Q4 FY26", "fourth quarter of fiscal 2024").
 * Fiscal periods are never re-labelled or mapped to calendar periods; this returns its input unchanged and exists
 * so every component goes through one place, which the tests pin down.
 */
export function formatPeriodLabel(label: string): string {
  return label;
}
