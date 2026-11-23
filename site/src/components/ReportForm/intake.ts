// What the report-an-error form sends to the intake (POST /v1/reports, contracts/cloud/report.schema.json): the form's
// field names are the reader's (`what`, `where`, `email`, `page`), the intake's are `text`, `where`, `email`, `page_id`.
// Pure functions, so the mapping, the limits and the page-ID rule are tested without a browser.
import report from "../../../../contracts/cloud/report.schema.json";
import type { ReportErrors, ReportValues } from "./ReportForm";

export const LIMITS = {
  textMin: report.$defs.text.minLength,
  textMax: report.$defs.text.maxLength,
  whereMax: report.$defs.where.maxLength,
  /** The API also caps the UTF-8 size (workers/api/src/reports.ts): one character can be 4 bytes. */
  textBytes: 4000,
  whereBytes: 600,
};

const PAGE_ID = /^[a-z0-9](?:[a-z0-9-]{0,118}[a-z0-9])?$/;
const bytes = (s: string) => new TextEncoder().encode(s).length;

/**
 * The page ID of what the reader gave: an ID, or the address of a page on this site (`/2026/11/<id>`, with or without
 * the origin, a trailing slash or `index.md`). The last path segment of a page's address is its ID. Undefined when it
 * is neither.
 */
export function pageIdOf(input: string, siteOrigin: string): string | undefined {
  const text = input.trim();
  if (PAGE_ID.test(text)) return text;
  let url: URL;
  try {
    url = new URL(text, siteOrigin);
  } catch {
    return undefined;
  }
  if (url.origin !== new URL(siteOrigin).origin) return undefined;
  const segments = url.pathname.split("/").filter(Boolean);
  if (/^index\.(?:md|html)$/.test(segments.at(-1) ?? "") || segments.at(-1) === "data.json") segments.pop();
  if (segments.length !== 3 || !/^\d{4}$/.test(segments[0] as string) || !/^\d{2}$/.test(segments[1] as string))
    return undefined;
  const id = segments[2] as string;
  return PAGE_ID.test(id) ? id : undefined;
}

/** A site path the sent confirmation can link back to: the page's own address, or "/" when it was an ID. */
export function backPath(input: string, siteOrigin: string): string {
  const id = pageIdOf(input, siteOrigin);
  if (!id || PAGE_ID.test(input.trim())) return "/";
  return new URL(input.trim(), siteOrigin).pathname;
}

/** The copy's messages for the limits (content/pages/report-an-error.md, "Form text"). */
export interface LimitMessages {
  textTooShort: string;
  textTooLong: string;
  whereTooLong: string;
}

/** The limits the intake enforces, checked before sending. Empty when none is broken. */
export function limitErrors(values: ReportValues, m: LimitMessages): ReportErrors {
  const errors: ReportErrors = {};
  const what = values.what?.trim() ?? "";
  if (what && [...what].length < LIMITS.textMin) errors.what = m.textTooShort;
  else if ([...what].length > LIMITS.textMax || bytes(what) > LIMITS.textBytes) errors.what = m.textTooLong;
  const where = values.where?.trim() ?? "";
  if ([...where].length > LIMITS.whereMax || bytes(where) > LIMITS.whereBytes) errors.where = m.whereTooLong;
  return errors;
}

export interface IntakeBody {
  page_id: string;
  text: string;
  where?: string;
  email?: string;
  website?: string;
}

/** The JSON the intake takes. Empty optional fields are left out; `website` is the honeypot, sent as the form had it. */
export function intakeBody(values: ReportValues, pageId: string, website = ""): IntakeBody {
  const where = values.where?.trim();
  const email = values.email?.trim();
  return {
    page_id: pageId,
    text: (values.what ?? "").trim(),
    ...(where ? { where } : {}),
    ...(email ? { email } : {}),
    ...(website ? { website } : {}),
  };
}
